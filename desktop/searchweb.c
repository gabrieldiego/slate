/*
 * Copyright 2014 Vincent Sanders <vince@netsurf-browser.org>
 *
 * This file is part of NetSurf, http://www.slate-browser.org/
 *
 * NetSurf is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; version 2 of the License.
 *
 * NetSurf is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

/**
 * \file
 * \brief core web search facilities implementation.
 */

#include <stdlib.h>
#include <string.h>

#include "utils/utils.h"
#include "utils/log.h"
#include "utils/url.h"
#include "utils/slateoption.h"
#include "slate/content.h"
#include "content/hlcache.h"

#include "desktop/searchweb.h"
#include "desktop/gui_internal.h"

struct search_provider {
	char *name; /**< readable name such as 'google', 'yahoo', etc */
	char *hostname; /**< host address such as www.google.com */
	char *searchstring; /** < such as "www.google.com?search=%s" */
	char *ico; /** < location of domain's favicon */
	hlcache_handle *ico_handle;
};

static struct search_web_ctx_s {
	struct search_provider *providers; /* web search providers */
	size_t providers_count; /* number of providers */

	size_t current; /* current provider */

	hlcache_handle *default_ico_handle;

} search_web_ctx;


static const char *default_providers = "DuckDuckGo|www.duckduckgo.com|https://www.duckduckgo.com/html/?q=%s|https://www.duckduckgo.com/favicon.ico|\n";

static const char *default_search_icon_url = "resource:icons/search.png";


/**
 * Read providers file.
 *
 * Allocates storage of sufficient size for the providers file and
 * reads the entire file in.
 *
 * \param fname The filename to read.
 * \param providers_out A pointer to place the result buffer in.
 * \param providers_size_out Size of buffer.
 * \return SLATEERROR_OK and providers_out updated or appropriate error code.
 */
static slateerror
read_providers(const char *fname,
	       char **providers_out,
	       size_t *providers_size_out)
{
	FILE *providersf;
	long ftellsize;
	size_t fsize;
	char *providersd;

	if (fname == NULL) {
		return SLATEERROR_BAD_PARAMETER;
	}

	providersf = fopen(fname, "r");
	if (providersf == NULL) {
		return SLATEERROR_NOT_FOUND;
	}

	if (fseek(providersf, 0, SEEK_END) != 0) {
		fclose(providersf);
		return SLATEERROR_INVALID;
	}

	ftellsize = ftell(providersf);
	if (ftellsize < 0) {
		fclose(providersf);
		return SLATEERROR_INVALID;
	}
	fsize = ftellsize;

	if (fseek(providersf, 0, SEEK_SET) != 0) {
		fclose(providersf);
		return SLATEERROR_INVALID;
	}

	providersd = malloc(fsize + 1);
	if (providersd == NULL) {
		fclose(providersf);
		return SLATEERROR_NOMEM;
	}

	if (fread(providersd, 1, fsize, providersf) != fsize) {
		fclose(providersf);
		free(providersd);
		return SLATEERROR_BAD_SIZE;
	}
	providersd[fsize] = 0; /* ensure null terminated */

	fclose(providersf);

	*providers_out = providersd;
	*providers_size_out = fsize;

	return SLATEERROR_OK;
}

/**
 * parse search providers from a memory block.
 *
 * \param providersd The provider info data.
 * \param providers_size The size of the provider data.
 * \param providers_out The resulting provider array.
 * \param providers_count The number of providers in the output array.
 * \return SLATEERROR_OK on success or error code on failure.
 */
static slateerror
parse_providers(char *providersd,
		size_t providers_size,
		struct search_provider **providers_out,
		size_t *providers_count)
{
	size_t pcount = 0; /* number of providers */
	size_t pidx;
	char *nl = providersd;
	struct search_provider *providers;

	/* count newlines */
	while (nl != NULL) {
		nl = strchr(nl, '\n');
		if (nl != NULL) {
			nl++;
			pcount+=1;
		}
	}

	if (pcount == 0) {
		return SLATEERROR_INVALID;
	}

	providers = malloc(pcount * sizeof(*providers));
	if (providers == NULL) {
		return SLATEERROR_NOMEM;
	}

	nl = providersd;
	for (pidx = 0; pidx < pcount; pidx++) {
		providers[pidx].name = nl;
		nl = strchr(nl, '|');
		if (nl == NULL) {
			free(providers);
			return SLATEERROR_INVALID;
		}
		*nl = 0;
		nl++;

		providers[pidx].hostname = nl;
		nl = strchr(nl, '|');
		if (nl == NULL) {
			free(providers);
			return SLATEERROR_INVALID;
		}
		*nl = 0;
		nl++;

		providers[pidx].searchstring = nl;
		nl = strchr(nl, '|');
		if (nl == NULL) {
			free(providers);
			return SLATEERROR_INVALID;
		}
		*nl = 0;
		nl++;

		providers[pidx].ico = nl;
		nl = strchr(nl, '|');
		if (nl == NULL) {
			free(providers);
			return SLATEERROR_INVALID;
		}
		*nl = 0;
		nl++;

		/* skip newline */
		nl = strchr(nl, '\n');
		if (nl == NULL) {
			free(providers);
			return SLATEERROR_INVALID;
		}
		nl++;

		providers[pidx].ico_handle = NULL;
	}

	*providers_out = providers;
	*providers_count = pcount;

	return SLATEERROR_OK;
}

/**
 * create a url for a search provider and a term
 *
 * \param provider The provider to use.
 * \param term The term being searched for.
 * \param url_out The resulting url.
 * \return SLATEERROR_OK on success or appropriate error code.
 */
static slateerror
make_search_slateurl(struct search_provider *provider,
		const char *term,
		slateurl **url_out)
{
	slateerror ret;
	slateurl *url;
	char *eterm; /* escaped term */
	char *searchstr; /* the providers search string */
	char *urlstr; /* the escaped term substituted into the provider */
	char *urlstro;
	size_t urlstr_len;

	/* escape the search term and join it to the search url */
	ret = url_escape(term, true, NULL, &eterm);
	if (ret != SLATEERROR_OK) {
		return ret;
	}

	searchstr = provider->searchstring;

	urlstr_len = strlen(searchstr) + strlen(eterm)  + 1;
	urlstro = urlstr = malloc(urlstr_len);
	if (urlstr == NULL) {
		free(eterm);
		return SLATEERROR_NOMEM;
	}

	/* composite search url */
	for ( ; *searchstr != 0; searchstr++, urlstro++) {
		*urlstro = *searchstr;
		if ((*searchstr == '%') && (searchstr[1] == 's')) {
			searchstr++; /* skip % */
			memcpy(urlstro, eterm, strlen(eterm));
			urlstro += strlen(eterm) - 1;
		}
	}
	free(eterm);
	*urlstro = '\0'; /* ensure string is NULL-terminated */

	ret = slateurl_create(urlstr, &url);
	free(urlstr);
	if (ret != SLATEERROR_OK) {
		return ret;
	}

	*url_out = url;
	return SLATEERROR_OK;
}

/**
 * callback for hlcache icon fetch events.
 */
static slateerror
search_web_ico_callback(hlcache_handle *ico,
			const hlcache_event *event,
			void *pw)
{
	struct search_provider *provider = pw;

	switch (event->type) {

	case CONTENT_MSG_DONE:
		NSLOG(netsurf, INFO, "icon '%s' retrieved",
		      slateurl_access(hlcache_handle_get_url(ico)));
		guit->search_web->provider_update(provider->name,
						  content_get_bitmap(ico));
		break;

	case CONTENT_MSG_ERROR:
		NSLOG(netsurf, INFO, "icon %s error: %s",
		      slateurl_access(hlcache_handle_get_url(ico)),
		      event->data.errordata.errormsg);

		hlcache_handle_release(ico);
		/* clear reference to released handle */
		provider->ico_handle = NULL;
		break;

	default:
		break;
	}

	return SLATEERROR_OK;
}

/* exported interface documented in desktop/searchweb.h */
slateerror
search_web_omni(const char *term,
		enum search_web_omni_flags flags,
		struct slateurl **url_out)
{
	slateerror ret;
	slateurl *url;
	size_t etermsize;
	char *eterm; /* encoded/altered search term */

	if ((flags & SEARCH_WEB_OMNI_SEARCHONLY) == 0) {

		/* first check to see if the term is a url */
		ret = slateurl_create(term, &url);
		if (ret == SLATEERROR_OK) {
			*url_out = url;
			return SLATEERROR_OK;
		}

		/* try with adding default scheme */
		etermsize = strlen(term) + SLEN("https://") + 1;
		eterm = malloc(etermsize);
		if (eterm == NULL) {
			return SLATEERROR_NOMEM;
		}
		snprintf(eterm, etermsize, "https://%s", term);
		ret = slateurl_create(eterm, &url);
		free(eterm);
		if (ret == SLATEERROR_OK) {
			*url_out = url;
			return SLATEERROR_OK;
		}

		/* do not pass to search if user has disabled the option */
		if (slateoption_bool(search_url_bar) == false) {
			return SLATEERROR_BAD_URL;
		}
	}

	/* ensure providers are initialised */
	if (search_web_ctx.providers == NULL) {
		ret = search_web_init(NULL);
		if (ret != SLATEERROR_OK) {
			return ret;
		}
	}
	if (search_web_ctx.providers == NULL) {
		return SLATEERROR_INIT_FAILED;
	}

	/* turn search into a slateurl */
	ret = make_search_slateurl(&search_web_ctx.providers[search_web_ctx.current], term, &url);
	if (ret != SLATEERROR_OK) {
		return ret;
	}

	*url_out = url;
	return SLATEERROR_OK;
}

/* exported interface documented in desktop/searchweb.h */
slateerror search_web_get_provider_bitmap(struct bitmap **bitmap_out)
{
	struct search_provider *provider;
	struct bitmap *ico_bitmap = NULL;

	/* must be initialised */
	if (search_web_ctx.providers == NULL) {
		return SLATEERROR_INIT_FAILED;
	}

	provider = &search_web_ctx.providers[search_web_ctx.current];

	/* set the icon now (if we can) at least to the default */
	if (provider->ico_handle != NULL) {
		ico_bitmap = content_get_bitmap(provider->ico_handle);
	}
	if ((ico_bitmap == NULL) &&
	    (search_web_ctx.default_ico_handle != NULL)) {
		ico_bitmap = content_get_bitmap(search_web_ctx.default_ico_handle);
	}

	*bitmap_out = ico_bitmap;
	return SLATEERROR_OK;
}


/* exported interface documented in desktop/searchweb.h */
slateerror search_web_select_provider(const char *selection)
{
	struct search_provider *provider;
	struct bitmap *ico_bitmap = NULL;
	size_t pidx;

	/* must be initialised */
	if (search_web_ctx.providers == NULL) {
		return SLATEERROR_INIT_FAILED;
	}

	/* negative value just selects whatevers current */
	if (selection != NULL) {
		for (pidx=0; pidx < search_web_ctx.providers_count;pidx++) {
			if (strcmp(search_web_ctx.providers[pidx].name, selection)==0) {
				search_web_ctx.current = pidx;
				break;
			}
		}
		if (pidx == search_web_ctx.providers_count) {
			/* selected provider not found */
			search_web_ctx.current = 0;
		}
	}

	provider = &search_web_ctx.providers[search_web_ctx.current];

	/* set the icon now (if we can) at least to the default */
	if (provider->ico_handle != NULL) {
		ico_bitmap = content_get_bitmap(provider->ico_handle);
	}
	if ((ico_bitmap == NULL) &&
	    (search_web_ctx.default_ico_handle != NULL)) {
		ico_bitmap = content_get_bitmap(search_web_ctx.default_ico_handle);
	}

	/* signal the frontend with the provider change. Bitmap may
	 * be NULL at this point.
	 */
	guit->search_web->provider_update(provider->name, ico_bitmap);

	/* if the providers icon has not been retrieved get it now */
	if (provider->ico_handle == NULL) {
		slateurl *icon_slateurl;
		slateerror ret;

		/* create search icon url */
		ret = slateurl_create(provider->ico, &icon_slateurl);
		if (ret != SLATEERROR_OK) {
			return ret;
		}

		ret = hlcache_handle_retrieve(icon_slateurl, 0, NULL, NULL,
					      search_web_ico_callback,
					      provider,
					      NULL, CONTENT_IMAGE,
					      &provider->ico_handle);
		slateurl_unref(icon_slateurl);
		if (ret != SLATEERROR_OK) {
			provider->ico_handle = NULL;
			return ret;
		}
	}

	return SLATEERROR_OK;
}

/**
 * callback for hlcache icon fetch events.
 */
static slateerror
default_ico_callback(hlcache_handle *ico,
		     const hlcache_event *event,
		     void *pw)
{
	struct search_web_ctx_s *ctx = pw;

	switch (event->type) {

	case CONTENT_MSG_DONE:
		NSLOG(netsurf, INFO, "default icon '%s' retrieved",
		      slateurl_access(hlcache_handle_get_url(ico)));

		/* only set to default icon if providers icon has no handle */
		if (ctx->providers[search_web_ctx.current].ico_handle == NULL) {
			guit->search_web->provider_update(
				ctx->providers[search_web_ctx.current].name,
				content_get_bitmap(ico));
		}
		break;

	case CONTENT_MSG_ERROR:
		NSLOG(netsurf, INFO, "icon %s error: %s",
		      slateurl_access(hlcache_handle_get_url(ico)),
		      event->data.errordata.errormsg);

		hlcache_handle_release(ico);
		/* clear reference to released handle */
		ctx->default_ico_handle = NULL;
		break;

	default:
		break;
	}

	return SLATEERROR_OK;
}

/* exported interface documented in desktop/searchweb.h */
ssize_t search_web_iterate_providers(ssize_t iter, const char **name)
{
	if (iter < 0) {
		iter=0;
	} else {
		iter++;
	}

	if ((size_t)iter >= search_web_ctx.providers_count) {
		iter = -1;
	} else {
		*name = search_web_ctx.providers[iter].name;
	}

	return iter;
}


/* exported interface documented in desktop/searchweb.h */
slateerror search_web_init(const char *provider_fname)
{
	slateerror ret;
	char *providers;
	size_t providers_size;
	slateurl *icon_slateurl;

	/* create search icon url */
	ret = slateurl_create(default_search_icon_url, &icon_slateurl);
	if (ret != SLATEERROR_OK) {
		return ret;
	}

	/* get a list of providers */
	ret = read_providers(provider_fname, &providers, &providers_size);
	if (ret != SLATEERROR_OK) {
		providers = strdup(default_providers);
		if (providers == NULL) {
			return SLATEERROR_NOMEM;
		}
		providers_size = strlen(providers);
	}

	/* parse list of providers */
	ret = parse_providers(providers,
			      providers_size,
			      &search_web_ctx.providers,
			      &search_web_ctx.providers_count);
	if (ret != SLATEERROR_OK) {
		free(providers);
		return ret;
	}

	/* get default search icon */
	ret = hlcache_handle_retrieve(icon_slateurl,
				      0,
				      NULL,
				      NULL,
				      default_ico_callback,
				      &search_web_ctx,
				      NULL,
				      CONTENT_IMAGE,
				      &search_web_ctx.default_ico_handle);
	slateurl_unref(icon_slateurl);
	if (ret != SLATEERROR_OK) {
		search_web_ctx.default_ico_handle = NULL;
		free(search_web_ctx.providers);
		search_web_ctx.providers = NULL;
		free(providers);
		return ret;
	}


	return SLATEERROR_OK;
}

/* exported interface documented in desktop/searchweb.h */
slateerror search_web_finalise(void)
{
	size_t pidx;

	/* must be initialised */
	if (search_web_ctx.providers == NULL) {
		return SLATEERROR_INIT_FAILED;
	}

	if (search_web_ctx.default_ico_handle != NULL) {
		hlcache_handle_release(search_web_ctx.default_ico_handle);
	}
	for (pidx = 0; pidx < search_web_ctx.providers_count; pidx++) {
		if (search_web_ctx.providers[pidx].ico_handle != NULL) {
			hlcache_handle_release(search_web_ctx.providers[pidx].ico_handle);
		}
	}

	/* All the search provider data is held in a single block for
	 * efficiency.
	 */
	free(search_web_ctx.providers[0].name);

	free(search_web_ctx.providers);
	search_web_ctx.providers = NULL;

	return SLATEERROR_OK;
}
