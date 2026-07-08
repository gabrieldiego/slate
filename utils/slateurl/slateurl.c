/*
 * Copyright 2011 Michael Drake <tlsa@netsurf-browser.org>
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
 * NetSurf URL handling implementation.
 *
 * This is the common implementation of all URL handling within the
 * browser. This implementation is based upon RFC3986 although this has
 * been superceeded by https://url.spec.whatwg.org/ which is based on
 * actual contemporary implementations.
 *
 * Care must be taken with character encodings within this module as
 * the specifications work with specific ascii ranges and must not be
 * affected by locale. Hence the c library character type functions
 * are not used.
 */

#include <assert.h>
#include <libwapcaplet/libwapcaplet.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <inttypes.h>

#include "utils/ascii.h"
#include "utils/corestrings.h"
#include "utils/errors.h"
#include "utils/idna.h"
#include "utils/log.h"
#include "utils/slateurl/private.h"
#include "utils/slateurl.h"
#include "utils/utils.h"


/**
 * Compare two component values.
 *
 * Sets match to false if the components are not the same.
 * Does nothing if the components are the same, so ensure match is
 * preset to true.
 */
#define slateurl__component_compare(c1, c2, match)			\
	if (c1 && c2 && lwc_error_ok ==				\
			lwc_string_isequal(c1, c2, match)) {	\
		/* do nothing */                                \
	} else if (c1 || c2) {					\
		*match = false;					\
	}



/******************************************************************************
 * NetSurf URL Public API                                                     *
 ******************************************************************************/

/* exported interface, documented in slateurl.h */
slateurl *slateurl_ref(slateurl *url)
{
	assert(url != NULL);

	url->count++;

	return url;
}


/* exported interface, documented in slateurl.h */
void slateurl_unref(slateurl *url)
{
	assert(url != NULL);
	assert(url->count > 0);

	if (--url->count > 0)
		return;

	/* Release lwc strings */
	slateurl__components_destroy(&url->components);

	/* Free the NetSurf URL */
	free(url);
}


/* exported interface, documented in slateurl.h */
bool slateurl_compare(const slateurl *url1, const slateurl *url2, slateurl_component parts)
{
	bool match = true;

	assert(url1 != NULL);
	assert(url2 != NULL);

	/* Compare URL components */

	/* Path, host and query first, since they're most likely to differ */

	if (parts & SLATEURL_PATH) {
		slateurl__component_compare(url1->components.path,
				url2->components.path, &match);

		if (match == false)
			return false;
	}

	if (parts & SLATEURL_HOST) {
		slateurl__component_compare(url1->components.host,
				url2->components.host, &match);

		if (match == false)
			return false;
	}

	if (parts & SLATEURL_QUERY) {
		slateurl__component_compare(url1->components.query,
				url2->components.query, &match);

		if (match == false)
			return false;
	}

	if (parts & SLATEURL_SCHEME) {
		slateurl__component_compare(url1->components.scheme,
				url2->components.scheme, &match);

		if (match == false)
			return false;
	}

	if (parts & SLATEURL_USERNAME) {
		slateurl__component_compare(url1->components.username,
				url2->components.username, &match);

		if (match == false)
			return false;
	}

	if (parts & SLATEURL_PASSWORD) {
		slateurl__component_compare(url1->components.password,
				url2->components.password, &match);

		if (match == false)
			return false;
	}

	if (parts & SLATEURL_PORT) {
		slateurl__component_compare(url1->components.port,
				url2->components.port, &match);

		if (match == false)
			return false;
	}

	if (parts & SLATEURL_FRAGMENT) {
		slateurl__component_compare(url1->components.fragment,
				url2->components.fragment, &match);

		if (match == false)
			return false;
	}

	return true;
}


/* exported interface, documented in slateurl.h */
slateerror slateurl_get(const slateurl *url, slateurl_component parts,
		char **url_s, size_t *url_l)
{
	assert(url != NULL);

	return slateurl__components_to_string(&(url->components), parts, 0,
			url_s, url_l);
}


/* exported interface, documented in slateurl.h */
lwc_string *slateurl_get_component(const slateurl *url, slateurl_component part)
{
	assert(url != NULL);

	switch (part) {
	case SLATEURL_SCHEME:
		return (url->components.scheme != NULL) ?
				lwc_string_ref(url->components.scheme) : NULL;

	case SLATEURL_USERNAME:
		return (url->components.username != NULL) ?
				lwc_string_ref(url->components.username) : NULL;

	case SLATEURL_PASSWORD:
		return (url->components.password != NULL) ?
				lwc_string_ref(url->components.password) : NULL;

	case SLATEURL_HOST:
		return (url->components.host != NULL) ?
				lwc_string_ref(url->components.host) : NULL;

	case SLATEURL_PORT:
		return (url->components.port != NULL) ?
				lwc_string_ref(url->components.port) : NULL;

	case SLATEURL_PATH:
		return (url->components.path != NULL) ?
				lwc_string_ref(url->components.path) : NULL;

	case SLATEURL_QUERY:
		return (url->components.query != NULL) ?
				lwc_string_ref(url->components.query) : NULL;

	case SLATEURL_FRAGMENT:
		return (url->components.fragment != NULL) ?
				lwc_string_ref(url->components.fragment) : NULL;

	default:
		NSLOG(netsurf, INFO,
		      "Unsupported value passed to part param.");
		assert(0);
	}

	return NULL;
}


/* exported interface, documented in slateurl.h */
enum slateurl_scheme_type slateurl_get_scheme_type(const slateurl *url)
{
	assert(url != NULL);

	return url->components.scheme_type;
}


/* exported interface, documented in slateurl.h */
bool slateurl_has_component(const slateurl *url, slateurl_component part)
{
	assert(url != NULL);

	switch (part) {
	case SLATEURL_SCHEME:
		if (url->components.scheme != NULL)
			return true;
		else
			return false;

	case SLATEURL_CREDENTIALS:
		/* Only username required for credentials section */
		/* Fall through */
	case SLATEURL_USERNAME:
		if (url->components.username != NULL)
			return true;
		else
			return false;

	case SLATEURL_PASSWORD:
		if (url->components.password != NULL)
			return true;
		else
			return false;

	case SLATEURL_HOST:
		if (url->components.host != NULL)
			return true;
		else
			return false;

	case SLATEURL_PORT:
		if (url->components.port != NULL)
			return true;
		else
			return false;

	case SLATEURL_PATH:
		if (url->components.path != NULL)
			return true;
		else
			return false;

	case SLATEURL_QUERY:
		if (url->components.query != NULL)
			return true;
		else
			return false;

	case SLATEURL_FRAGMENT:
		if (url->components.fragment != NULL)
			return true;
		else
			return false;

	default:
		NSLOG(netsurf, INFO,
		      "Unsupported value passed to part param.");
		assert(0);
	}

	return false;
}


/* exported interface, documented in slateurl.h */
const char *slateurl_access(const slateurl *url)
{
	assert(url != NULL);

	return url->string;
}


/* exported interface, documented in slateurl.h */
const char *slateurl_access_log(const slateurl *url)
{
	assert(url != NULL);

	if (url->components.scheme_type == SLATEURL_SCHEME_DATA) {
		return "[data url]";
	}

	return url->string;
}


/* exported interface, documented in slateurl.h */
slateerror slateurl_get_utf8(const slateurl *url, char **url_s, size_t *url_l)
{
	slateerror err;
	lwc_string *host;
	char *idna_host = NULL;
	size_t idna_host_len;
	char *scheme = NULL;
	size_t scheme_len;
	char *path = NULL;
	size_t path_len;
	char *url_out; /* url string */
	char *url_cur; /* url cursor */

	assert(url != NULL);

	if (url->components.host == NULL) {
		return slateurl_get(url, SLATEURL_WITH_FRAGMENT, url_s, url_l);
	}

	host = url->components.host;
	err = idna_decode(lwc_string_data(host), lwc_string_length(host),
			&idna_host, &idna_host_len);
	if (err != SLATEERROR_OK) {
		/* Fall back to non-IDNA */
		idna_host_len = lwc_string_length(host);
		idna_host = malloc(idna_host_len + 1);
		if (idna_host == NULL) {
			err = SLATEERROR_NOMEM;
			goto cleanup;
		}
		memcpy(idna_host, lwc_string_data(host), idna_host_len);
		idna_host[idna_host_len] = '\0';
	}

	err = slateurl_get(url,
			SLATEURL_SCHEME | SLATEURL_CREDENTIALS,
			&scheme, &scheme_len);
	if (err != SLATEERROR_OK) {
		goto cleanup;
	}

	err = slateurl_get(url,
			SLATEURL_PORT | SLATEURL_PATH | SLATEURL_QUERY | SLATEURL_FRAGMENT,
			&path, &path_len);
	if (err != SLATEERROR_OK) {
		goto cleanup;
	}

	/* allocate space for output url string allowing for terminator */
	url_out = url_cur = malloc(scheme_len + idna_host_len + path_len + 1);
	if (url_out == NULL) {
		err = SLATEERROR_NOMEM;
		goto cleanup;
	}

	/* copy the three parts into output buffer and null terminate */
	memcpy(url_cur, scheme, scheme_len);
	url_cur += scheme_len;
	memcpy(url_cur, idna_host, idna_host_len);
	url_cur += idna_host_len;
	memcpy(url_cur, path, path_len);
	url_cur += path_len;
	*url_cur = 0;

	*url_l = url_cur - url_out;
	*url_s = url_out;
	err = SLATEERROR_OK;

cleanup:
	free(idna_host);
	free(scheme);
	free(path);

	return err;
}


/* exported interface, documented in slateurl.h */
const char *slateurl_access_leaf(const slateurl *url)
{
	size_t path_len;
	const char *path;
	const char *leaf;

	assert(url != NULL);

	if (url->components.path == NULL)
		return "";

	path = lwc_string_data(url->components.path);
	path_len = lwc_string_length(url->components.path);

	if (path_len == 0)
		return "";

	if (path_len == 1 && *path == '/')
		return "/";

	leaf = path + path_len;

	do {
		leaf--;
	} while ((leaf != path) && (*leaf != '/'));

	if (*leaf == '/')
		leaf++;

	return leaf;
}


/* exported interface, documented in slateurl.h */
size_t slateurl_length(const slateurl *url)
{
	assert(url != NULL);

	return url->length;
}


/* exported interface, documented in slateurl.h */
uint32_t slateurl_hash(const slateurl *url)
{
	assert(url != NULL);

	return url->hash;
}


/* exported interface, documented in slateurl.h */
slateerror slateurl_defragment(const slateurl *url, slateurl **no_frag)
{
	size_t length;
	char *pos;

	assert(url != NULL);

	/* check for source url having no fragment already */
	if (url->components.fragment == NULL) {
		*no_frag = (slateurl *)url;

		(*no_frag)->count++;

		return SLATEERROR_OK;
	}

	/* Find the change in length from url to new_url */
	length = url->length;
	if (url->components.fragment != NULL) {
		length -= 1 + lwc_string_length(url->components.fragment);
	}

	/* Create NetSurf URL object */
	*no_frag = malloc(sizeof(slateurl) + length + 1); /* Add 1 for \0 */
	if (*no_frag == NULL) {
		return SLATEERROR_NOMEM;
	}

	/* Copy components */
	(*no_frag)->components.scheme =
			slateurl__component_copy(url->components.scheme);
	(*no_frag)->components.username =
			slateurl__component_copy(url->components.username);
	(*no_frag)->components.password =
			slateurl__component_copy(url->components.password);
	(*no_frag)->components.host =
			slateurl__component_copy(url->components.host);
	(*no_frag)->components.port =
			slateurl__component_copy(url->components.port);
	(*no_frag)->components.path =
			slateurl__component_copy(url->components.path);
	(*no_frag)->components.query =
			slateurl__component_copy(url->components.query);
	(*no_frag)->components.fragment = NULL;

	(*no_frag)->components.scheme_type = url->components.scheme_type;

	(*no_frag)->length = length;

	/* Fill out the url string */
	pos = (*no_frag)->string;
	memcpy(pos, url->string, length);
	pos += length;
	*pos = '\0';

	/* Get the slateurl's hash */
	slateurl__calc_hash(*no_frag);

	/* Give the URL a reference */
	(*no_frag)->count = 1;

	return SLATEERROR_OK;
}


/* exported interface, documented in slateurl.h */
slateerror slateurl_refragment(const slateurl *url, lwc_string *frag, slateurl **new_url)
{
	int frag_len;
	int base_len;
	char *pos;
	size_t len;

	assert(url != NULL);
	assert(frag != NULL);

	/* Find the change in length from url to new_url */
	base_len = url->length;
	if (url->components.fragment != NULL) {
		base_len -= 1 + lwc_string_length(url->components.fragment);
	}
	frag_len = lwc_string_length(frag);

	/* Set new_url's length */
	len = base_len + 1 /* # */ + frag_len;

	/* Create NetSurf URL object */
	*new_url = malloc(sizeof(slateurl) + len + 1); /* Add 1 for \0 */
	if (*new_url == NULL) {
		return SLATEERROR_NOMEM;
	}

	(*new_url)->length = len;

	/* Set string */
	pos = (*new_url)->string;
	memcpy(pos, url->string, base_len);
	pos += base_len;
	*pos = '#';
	memcpy(++pos, lwc_string_data(frag), frag_len);
	pos += frag_len;
	*pos = '\0';

	/* Copy components */
	(*new_url)->components.scheme =
			slateurl__component_copy(url->components.scheme);
	(*new_url)->components.username =
			slateurl__component_copy(url->components.username);
	(*new_url)->components.password =
			slateurl__component_copy(url->components.password);
	(*new_url)->components.host =
			slateurl__component_copy(url->components.host);
	(*new_url)->components.port =
			slateurl__component_copy(url->components.port);
	(*new_url)->components.path =
			slateurl__component_copy(url->components.path);
	(*new_url)->components.query =
			slateurl__component_copy(url->components.query);
	(*new_url)->components.fragment =
			lwc_string_ref(frag);

	(*new_url)->components.scheme_type = url->components.scheme_type;

	/* Get the slateurl's hash */
	slateurl__calc_hash(*new_url);

	/* Give the URL a reference */
	(*new_url)->count = 1;

	return SLATEERROR_OK;
}


/* exported interface, documented in slateurl.h */
slateerror slateurl_replace_query(const slateurl *url, const char *query,
		slateurl **new_url)
{
	int query_len;    /* Length of new query string excluding '?' */
	int frag_len = 0; /* Length of fragment, excluding '#' */
	int base_len;     /* Length of URL up to start of query */
	char *pos;        /* current position in output string */
	size_t length;    /* new url string length */
	lwc_string *lwc_query = NULL;

	assert(url != NULL);
	assert(query != NULL);

	length = query_len = strlen(query);
	if (query_len > 0) {
		length++; /* allow for '?' */

		/* intern string */
		if (lwc_intern_string(query,
				      query_len,
				      &lwc_query) != lwc_error_ok) {
			return SLATEERROR_NOMEM;
		}
	}

	/* Find the change in length from url to new_url */
	base_len = url->length;
	if (url->components.query != NULL) {
		base_len -= (1 + lwc_string_length(url->components.query));
	}
	if (url->components.fragment != NULL) {
		frag_len = lwc_string_length(url->components.fragment);
		base_len -= (1 + frag_len);
		length += frag_len + 1; /* allow for '#' */
	}

	/* compute new url string length */
	length += base_len;

	/* Create NetSurf URL object */
	*new_url = malloc(sizeof(slateurl) + length + 1); /* Add 1 for \0 */
	if (*new_url == NULL) {
		lwc_string_unref(lwc_query);
		return SLATEERROR_NOMEM;
	}

	(*new_url)->length = length;

	/* Set string */
	pos = (*new_url)->string;
	memcpy(pos, url->string, base_len);
	pos += base_len;
	if (query_len > 0) {
		*pos = '?';
		memcpy(++pos, query, query_len);
		pos += query_len;
	}
	if (url->components.fragment != NULL) {
		const char *frag = lwc_string_data(url->components.fragment);
		*pos = '#';
		memcpy(++pos, frag, frag_len);
		pos += frag_len;
	}
	*pos = '\0';

	/* Copy components */
	(*new_url)->components.scheme =
			slateurl__component_copy(url->components.scheme);
	(*new_url)->components.username =
			slateurl__component_copy(url->components.username);
	(*new_url)->components.password =
			slateurl__component_copy(url->components.password);
	(*new_url)->components.host =
			slateurl__component_copy(url->components.host);
	(*new_url)->components.port =
			slateurl__component_copy(url->components.port);
	(*new_url)->components.path =
			slateurl__component_copy(url->components.path);
	(*new_url)->components.query = lwc_query;
	(*new_url)->components.fragment =
			slateurl__component_copy(url->components.fragment);

	(*new_url)->components.scheme_type = url->components.scheme_type;

	/* Get the slateurl's hash */
	slateurl__calc_hash(*new_url);

	/* Give the URL a reference */
	(*new_url)->count = 1;

	return SLATEERROR_OK;
}


/* exported interface, documented in slateurl.h */
slateerror slateurl_replace_scheme(const slateurl *url, lwc_string *scheme,
		slateurl **new_url)
{
	int scheme_len;
	int base_len;
	char *pos;
	size_t len;
	bool match;

	assert(url != NULL);
	assert(scheme != NULL);

	/* Get the length of the new scheme */
	scheme_len = lwc_string_length(scheme);

	/* Find the change in length from url to new_url */
	base_len = url->length;
	if (url->components.scheme != NULL) {
		base_len -= lwc_string_length(url->components.scheme);
	}

	/* Set new_url's length */
	len = base_len + scheme_len;

	/* Create NetSurf URL object */
	*new_url = malloc(sizeof(slateurl) + len + 1); /* Add 1 for \0 */
	if (*new_url == NULL) {
		return SLATEERROR_NOMEM;
	}

	(*new_url)->length = len;

	/* Set string */
	pos = (*new_url)->string;
	memcpy(pos, lwc_string_data(scheme), scheme_len);
	memcpy(pos + scheme_len,
			url->string + url->length - base_len, base_len);
	pos[len] = '\0';

	/* Copy components */
	(*new_url)->components.scheme = lwc_string_ref(scheme);
	(*new_url)->components.username =
			slateurl__component_copy(url->components.username);
	(*new_url)->components.password =
			slateurl__component_copy(url->components.password);
	(*new_url)->components.host =
			slateurl__component_copy(url->components.host);
	(*new_url)->components.port =
			slateurl__component_copy(url->components.port);
	(*new_url)->components.path =
			slateurl__component_copy(url->components.path);
	(*new_url)->components.query =
			slateurl__component_copy(url->components.query);
	(*new_url)->components.fragment =
			slateurl__component_copy(url->components.fragment);

	/* Compute new scheme type */
	if (lwc_string_caseless_isequal(scheme, corestring_lwc_http,
			&match) == lwc_error_ok && match == true) {
		(*new_url)->components.scheme_type = SLATEURL_SCHEME_HTTP;
	} else if (lwc_string_caseless_isequal(scheme, corestring_lwc_https,
			&match) == lwc_error_ok && match == true) {
		(*new_url)->components.scheme_type = SLATEURL_SCHEME_HTTPS;
	} else if (lwc_string_caseless_isequal(scheme, corestring_lwc_file,
			&match) == lwc_error_ok && match == true) {
		(*new_url)->components.scheme_type = SLATEURL_SCHEME_FILE;
	} else if (lwc_string_caseless_isequal(scheme, corestring_lwc_ftp,
			&match) == lwc_error_ok && match == true) {
		(*new_url)->components.scheme_type = SLATEURL_SCHEME_FTP;
	} else if (lwc_string_caseless_isequal(scheme, corestring_lwc_mailto,
			&match) == lwc_error_ok && match == true) {
		(*new_url)->components.scheme_type = SLATEURL_SCHEME_MAILTO;
	} else {
		(*new_url)->components.scheme_type = SLATEURL_SCHEME_OTHER;
	}

	/* Get the slateurl's hash */
	slateurl__calc_hash(*new_url);

	/* Give the URL a reference */
	(*new_url)->count = 1;

	return SLATEERROR_OK;
}


/* exported interface documented in utils/slateurl.h */
slateerror slateurl_nice(const slateurl *url, char **result, bool remove_extensions)
{
	const char *data;
	size_t len;
	size_t pos;
	bool match;
	char *name;

	assert(url != NULL);

	*result = 0;

	/* extract the last component of the path, if possible */
	if ((url->components.path != NULL) &&
	    (lwc_string_length(url->components.path) != 0) &&
	    (lwc_string_isequal(url->components.path,
			corestring_lwc_slash_, &match) == lwc_error_ok) &&
	    (match == false)) {
		bool first = true;
		bool keep_looking;

		/* Get hold of the string data we're examining */
		data = lwc_string_data(url->components.path);
		len = lwc_string_length(url->components.path);
		pos = len;

		do {
			keep_looking = false;
			pos--;

			/* Find last '/' with stuff after it */
			while (pos != 0) {
				if (data[pos] == '/' && pos < len - 1) {
					break;
				}
				pos--;
			}

			if (pos == 0) {
				break;
			}

			if (first) {
				if (strncasecmp("/default.", data + pos,
						SLEN("/default.")) == 0) {
					keep_looking = true;

				} else if (strncasecmp("/index.",
							data + pos,
							6) == 0) {
					keep_looking = true;

				}
				first = false;
			}

		} while (keep_looking);

		if (data[pos] == '/')
			pos++;

		if (strncasecmp("default.", data + pos, 8) != 0 &&
				strncasecmp("index.", data + pos, 6) != 0) {
			size_t end = pos;
			while (data[end] != '\0' && data[end] != '/') {
				end++;
			}
			if (end - pos != 0) {
				name = malloc(end - pos + 1);
				if (name == NULL) {
					return SLATEERROR_NOMEM;
				}
				memcpy(name, data + pos, end - pos);
				name[end - pos] = '\0';
				if (remove_extensions) {
					/* strip any extenstion */
					char *dot = strchr(name, '.');
					if (dot && dot != name) {
						*dot = '\0';
					}
				}
				*result = name;
				return SLATEERROR_OK;
			}
		}
	}

	if (url->components.host != NULL) {
		name = strdup(lwc_string_data(url->components.host));

		for (pos = 0; name[pos] != '\0'; pos++) {
			if (name[pos] == '.') {
				name[pos] = '_';
			}
		}

		*result = name;
		return SLATEERROR_OK;
	}

	return SLATEERROR_NOT_FOUND;
}


/* exported interface, documented in slateurl.h */
slateerror slateurl_parent(const slateurl *url, slateurl **new_url)
{
	lwc_string *lwc_path;
	size_t old_path_len, new_path_len;
	size_t len;
	const char* path = NULL;
	char *pos;

	assert(url != NULL);

	old_path_len = (url->components.path == NULL) ? 0 :
			lwc_string_length(url->components.path);

	/* Find new path length */
	if (old_path_len == 0) {
		new_path_len = old_path_len;
	} else {
		path = lwc_string_data(url->components.path);

		new_path_len = old_path_len;
		if (old_path_len > 1) {
			/* Skip over any trailing / */
			if (path[new_path_len - 1] == '/')
				new_path_len--;

			/* Work back to next / */
			while (new_path_len > 0 &&
					path[new_path_len - 1] != '/')
				new_path_len--;
		}
	}

	/* Find the length of new_url */
	len = url->length;
	if (url->components.query != NULL) {
		len -= lwc_string_length(url->components.query);
	}
	if (url->components.fragment != NULL) {
		len -= 1; /* # */
		len -= lwc_string_length(url->components.fragment);
	}
	len -= old_path_len - new_path_len;

	/* Create NetSurf URL object */
	*new_url = malloc(sizeof(slateurl) + len + 1); /* Add 1 for \0 */
	if (*new_url == NULL) {
		return SLATEERROR_NOMEM;
	}

	/* Make new path */
	if (old_path_len == 0) {
		lwc_path = NULL;
	} else if (old_path_len == new_path_len) {
		lwc_path = lwc_string_ref(url->components.path);
	} else {
		if (lwc_intern_string(path, new_path_len,
				&lwc_path) != lwc_error_ok) {
			free(*new_url);
			return SLATEERROR_NOMEM;
		}
	}

	(*new_url)->length = len;

	/* Set string */
	pos = (*new_url)->string;
	memcpy(pos, url->string, len);
	pos += len;
	*pos = '\0';

	/* Copy components */
	(*new_url)->components.scheme =
			slateurl__component_copy(url->components.scheme);
	(*new_url)->components.username =
			slateurl__component_copy(url->components.username);
	(*new_url)->components.password =
			slateurl__component_copy(url->components.password);
	(*new_url)->components.host =
			slateurl__component_copy(url->components.host);
	(*new_url)->components.port =
			slateurl__component_copy(url->components.port);
	(*new_url)->components.path = lwc_path;
	(*new_url)->components.query = NULL;
	(*new_url)->components.fragment = NULL;

	(*new_url)->components.scheme_type = url->components.scheme_type;

	/* Get the slateurl's hash */
	slateurl__calc_hash(*new_url);

	/* Give the URL a reference */
	(*new_url)->count = 1;

	return SLATEERROR_OK;
}

/* exported interface, documented in slateurl.h */
void slateurl_dump(const slateurl *url)
{
	fprintf(stderr, "slateurl components for %p "
			"(refs: %i hash: %"PRIx32"):\n",
			url, url->count, url->hash);

	if (url->components.scheme)
		fprintf(stderr, "  Scheme: %s\n",
				lwc_string_data(url->components.scheme));
	if (url->components.username)
		fprintf(stderr, "Username: %s\n",
				lwc_string_data(url->components.username));
	if (url->components.password)
		fprintf(stderr, "Password: %s\n",
				lwc_string_data(url->components.password));
	if (url->components.host)
		fprintf(stderr, "    Host: %s\n",
				lwc_string_data(url->components.host));
	if (url->components.port)
		fprintf(stderr, "    Port: %s\n",
				lwc_string_data(url->components.port));
	if (url->components.path)
		fprintf(stderr, "    Path: %s\n",
				lwc_string_data(url->components.path));
	if (url->components.query)
		fprintf(stderr, "   Query: %s\n",
				lwc_string_data(url->components.query));
	if (url->components.fragment)
		fprintf(stderr, "Fragment: %s\n",
				lwc_string_data(url->components.fragment));
}
