/*
 * Copyright 2012 Vincent Sanders <vince@netsurf-browser.org>
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
 * Option reading and saving (implementation).
 *
 * Options are stored in the format key:value, one per line.
 *
 * For bool options, value is "0" or "1".
 */

#include <stdio.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#include "slate/inttypes.h"
#include "slate/plot_style.h"
#include "utils/errors.h"
#include "utils/log.h"
#include "utils/utils.h"
#include "utils/slateoption.h"

/** Length of buffer used to read lines from input file */
#define SLATEOPTION_MAX_LINE_LEN 1024

struct slateoption_s *slateoptions = NULL;
struct slateoption_s *slateoptions_default = NULL;

#define SLATEOPTION_BOOL(NAME, DEFAULT) \
	{ #NAME, sizeof(#NAME) - 1, OPTION_BOOL, { .b = DEFAULT } },

#define SLATEOPTION_STRING(NAME, DEFAULT) \
	{ #NAME, sizeof(#NAME) - 1, OPTION_STRING, { .cs = DEFAULT } },

#define SLATEOPTION_INTEGER(NAME, DEFAULT) \
	{ #NAME, sizeof(#NAME) - 1, OPTION_INTEGER, { .i = DEFAULT } },

#define SLATEOPTION_UINT(NAME, DEFAULT) \
	{ #NAME, sizeof(#NAME) - 1, OPTION_UINT, { .u = DEFAULT } },

#define SLATEOPTION_COLOUR(NAME, DEFAULT) \
	{ #NAME, sizeof(#NAME) - 1, OPTION_COLOUR, { .c = DEFAULT } },

/** The table of compiled in default options */
static struct slateoption_s defaults[] = {
#include "desktop/options.h"

#if defined(riscos)
#include "riscos/options.h"
#elif defined(slategtk)
#include "gtk/options.h"
#elif defined(slatebeos)
#include "beos/options.h"
#elif defined(slateamiga)
#include "amiga/options.h"
#elif defined(slateframebuffer)
#include "framebuffer/options.h"
#elif defined(slateatari)
#include "atari/options.h"
#elif defined(jotter)
#include "monkey/options.h"
#elif defined(slatewin32)
#include "windows/options.h"
#elif defined(slateqt)
#include "qt/options.h"
#endif
	{ NULL, 0, OPTION_INTEGER, { 0 } }
};

#undef SLATEOPTION_BOOL
#undef SLATEOPTION_STRING
#undef SLATEOPTION_INTEGER
#undef SLATEOPTION_UINT
#undef SLATEOPTION_COLOUR

/**
 * Set an option value based on a string
 */
static bool
strtooption(const char *value, struct slateoption_s *option)
{
	bool ret = true;
	colour rgbcolour; /* RRGGBB */

	switch (option->type) {
	case OPTION_BOOL:
		option->value.b = (value[0] == '1');
		break;

	case OPTION_INTEGER:
		option->value.i = atoi(value);
		break;

	case OPTION_UINT:
		option->value.u = strtoul(value, NULL, 0);
		break;

	case OPTION_COLOUR:
		if (sscanf(value, "%"SCNx32"", &rgbcolour) == 1) {
			option->value.c = (((0x000000FF & rgbcolour) << 16) |
					   ((0x0000FF00 & rgbcolour) << 0) |
					   ((0x00FF0000 & rgbcolour) >> 16));
		}
		break;

	case OPTION_STRING:
		if (option->value.s != NULL) {
			free(option->value.s);
		}

		if (*value == 0) {
			/* do not allow empty strings in text options */
			option->value.s = NULL;
		} else {
			option->value.s = strdup(value);
		}
		break;

	default:
		ret = false;
		break;
	}

	return ret;
}

/* validate options to sane values */
static void slateoption_validate(struct slateoption_s *opts, struct slateoption_s *defs)
{
	int cloop;
	bool black = true;

	if (opts[SLATEOPTION_treeview_font_size].value.i  < 50) {
		opts[SLATEOPTION_treeview_font_size].value.i = 50;
	}

	if (opts[SLATEOPTION_treeview_font_size].value.i > 1000) {
		opts[SLATEOPTION_treeview_font_size].value.i = 1000;
	}

	if (opts[SLATEOPTION_font_size].value.i  < 50) {
		opts[SLATEOPTION_font_size].value.i = 50;
	}

	if (opts[SLATEOPTION_font_size].value.i > 1000) {
		opts[SLATEOPTION_font_size].value.i = 1000;
	}

	if (opts[SLATEOPTION_font_min_size].value.i < 10) {
		opts[SLATEOPTION_font_min_size].value.i = 10;
	}

	if (opts[SLATEOPTION_font_min_size].value.i > 500) {
		opts[SLATEOPTION_font_min_size].value.i = 500;
	}

	if (opts[SLATEOPTION_memory_cache_size].value.i < 0) {
		opts[SLATEOPTION_memory_cache_size].value.i = 0;
	}

	/* to aid migration from old, broken, configuration files this
	 * checks to see if all the system colours are set to black
	 * and returns them to defaults instead
	 */

	for (cloop = SLATEOPTION_SYS_COLOUR_START;
	     cloop <= SLATEOPTION_SYS_COLOUR_END;
	     cloop++) {
		if (opts[cloop].value.c != 0) {
			black = false;
			break;
		}
	}
	if (black == true && defs != NULL) {
		for (cloop = SLATEOPTION_SYS_COLOUR_START;
		     cloop <= SLATEOPTION_SYS_COLOUR_END;
		     cloop++) {
			opts[cloop].value.c = defs[cloop].value.c;
		}
	}

	/* To aid migration and ensure that timeouts don't go crazy,
	 * ensure that (a) we allow at least 1 attempt and
	 * (b) the total time that we spend should not exceed 60s
	 */
	if (opts[SLATEOPTION_max_retried_fetches].value.u == 0)
		opts[SLATEOPTION_max_retried_fetches].value.u = 1;
	if (opts[SLATEOPTION_curl_fetch_timeout].value.u < 5)
		opts[SLATEOPTION_curl_fetch_timeout].value.u = 5;
	if (opts[SLATEOPTION_curl_fetch_timeout].value.u > 60)
		opts[SLATEOPTION_curl_fetch_timeout].value.u = 60;
	while (((opts[SLATEOPTION_curl_fetch_timeout].value.u *
		 opts[SLATEOPTION_max_retried_fetches].value.u) > 60) &&
	       (opts[SLATEOPTION_max_retried_fetches].value.u > 1))
		opts[SLATEOPTION_max_retried_fetches].value.u--;

	/* We ignore the result because we can't fail to validate. Yay */
	(void)nslog_set_filter_by_options();
}

/**
 * Determines if an option is different between two option tables.
 *
 * @param opts The first table to compare.
 * @param defs The second table to compare.
 * @param entry The option to compare.
 * @return true if the option differs false if not.
 */
static bool
slateoption_is_set(const struct slateoption_s *opts,
		const struct slateoption_s *defs,
		const enum slateoption_e entry)
{
	bool ret = false;

	switch (opts[entry].type) {
	case OPTION_BOOL:
		if (opts[entry].value.b != defs[entry].value.b) {
			ret = true;
		}
		break;

	case OPTION_INTEGER:
		if (opts[entry].value.i != defs[entry].value.i) {
			ret = true;
		}
		break;

	case OPTION_UINT:
		if (opts[entry].value.u != defs[entry].value.u) {
			ret = true;
		}
		break;

	case OPTION_COLOUR:
		if (opts[entry].value.c != defs[entry].value.c) {
			ret = true;
		}
		break;

	case OPTION_STRING:
		/* set if:
		 *  - defs is null.
		 *  - default is null but value is not.
		 *  - default and value pointers are different
		 *    (acts as a null check because of previous check)
		 *    and the strings content differ.
		 */
		if (((defs[entry].value.s == NULL) &&
		     (opts[entry].value.s != NULL)) ||
		    ((defs[entry].value.s != NULL) &&
		     (opts[entry].value.s == NULL)) ||
		    ((defs[entry].value.s != opts[entry].value.s) &&
		     (strcmp(opts[entry].value.s, defs[entry].value.s) != 0))) {
			ret = true;
		}
		break;

	}
	return ret;
}

/**
 * Output an option value into a file stream, in plain text format.
 *
 * @param option The option to output the value of.
 * @param fp The file stream to write to.
 * @return The number of bytes written to string or -1 on error
 */
static size_t slateoption_output_value_file(struct slateoption_s *option, void *ctx)
{
	FILE *fp = ctx;
	size_t slen = 0; /* length added to stream */
	colour rgbcolour; /* RRGGBB */

	switch (option->type) {
	case OPTION_BOOL:
		slen = fprintf(fp, "%s:%c\n", option->key, option->value.b ? '1' : '0');
		break;

	case OPTION_INTEGER:
		slen = fprintf(fp, "%s:%i\n", option->key, option->value.i);

		break;

	case OPTION_UINT:
		slen = fprintf(fp, "%s:%u\n", option->key, option->value.u);
		break;

	case OPTION_COLOUR:
		rgbcolour = (((0x000000FF & option->value.c) << 16) |
			     ((0x0000FF00 & option->value.c) << 0) |
			     ((0x00FF0000 & option->value.c) >> 16));
		slen = fprintf(fp, "%s:%06"PRIx32"\n", option->key, rgbcolour);
		break;

	case OPTION_STRING:
		slen = fprintf(fp, "%s:%s\n",
			       option->key,
			       ((option->value.s == NULL) ||
				(*option->value.s == 0)) ? "" : option->value.s);
		break;
	}

	return slen;
}

/**
 * Output an option value into a string, in HTML format.
 *
 * @param option The option to output the value of.
 * @param size The size of the string buffer.
 * @param pos The current position in string
 * @param string The string in which to output the value.
 * @return The number of bytes written to string or -1 on error
 */
static size_t
slateoption_output_value_html(struct slateoption_s *option,
			   size_t size,
			   size_t pos,
			   char *string)
{
	size_t slen = 0; /* length added to string */
	colour rgbcolour; /* RRGGBB */

	switch (option->type) {
	case OPTION_BOOL:
		slen = snprintf(string + pos,
				size - pos,
				"%s",
				option->value.b ? "true" : "false");
		break;

	case OPTION_INTEGER:
		slen = snprintf(string + pos,
				size - pos,
				"%i",
				option->value.i);
		break;

	case OPTION_UINT:
		slen = snprintf(string + pos,
				size - pos,
				"%u",
				option->value.u);
		break;

	case OPTION_COLOUR:
		rgbcolour = colour_rb_swap(option->value.c);
		slen = snprintf(string + pos,
				size - pos,
				"<span style=\"font-family:Monospace;\">"
				"#%06"PRIX32
				"</span> "
				"<span style=\"background-color: #%06"PRIx32"; "
				"border: 1px solid #%06"PRIx32"; "
				"display: inline-block; "
				"width: 1em; height: 1em;\">"
				"</span>",
				rgbcolour,
				rgbcolour,
				colour_to_bw_furthest(rgbcolour));
		break;

	case OPTION_STRING:
		if (option->value.s != NULL) {
			slen = snprintf(string + pos, size - pos, "%s",
					option->value.s);
		} else {
			slen = snprintf(string + pos, size - pos,
					"<span class=\"null-content\">NULL"
					"</span>");
		}
		break;
	}

	return slen;
}


/**
 * Output an option value into a string, in plain text format.
 *
 * @param option The option to output the value of.
 * @param size The size of the string buffer.
 * @param pos The current position in string
 * @param string The string in which to output the value.
 * @return The number of bytes written to string or -1 on error
 */
static size_t
slateoption_output_value_text(struct slateoption_s *option,
			   size_t size,
			   size_t pos,
			   char *string)
{
	size_t slen = 0; /* length added to string */
	colour rgbcolour; /* RRGGBB */

	switch (option->type) {
	case OPTION_BOOL:
		slen = snprintf(string + pos,
				size - pos,
				"%c",
				option->value.b ? '1' : '0');
		break;

	case OPTION_INTEGER:
		slen = snprintf(string + pos,
				size - pos,
				"%i",
				option->value.i);
		break;

	case OPTION_UINT:
		slen = snprintf(string + pos,
				size - pos,
				"%u",
				option->value.u);
		break;

	case OPTION_COLOUR:
		rgbcolour = (((0x000000FF & option->value.c) << 16) |
			     ((0x0000FF00 & option->value.c) << 0) |
			     ((0x00FF0000 & option->value.c) >> 16));
		slen = snprintf(string + pos, size - pos, "%06"PRIx32, rgbcolour);
		break;

	case OPTION_STRING:
		if (option->value.s != NULL) {
			slen = snprintf(string + pos,
					size - pos,
					"%s",
					option->value.s);
		}
		break;
	}

	return slen;
}

/**
 * Duplicates an option table.
 *
 * Allocates a new option table and copies an existing one into it.
 *
 * \param[in] src The source table to copy
 * \param[out] pdst The output table
 * \return SLATEERROR_OK on success or appropriate error code.
 */
static slateerror
slateoption_dup(struct slateoption_s *src, struct slateoption_s **pdst)
{
	struct slateoption_s *dst;
	dst = malloc(sizeof(defaults));
	if (dst == NULL) {
		return SLATEERROR_NOMEM;
	}
	*pdst = dst;

	/* copy the source table into the destination table */
	memcpy(dst, src, sizeof(defaults));

	while (src->key != NULL) {
		if ((src->type == OPTION_STRING) &&
		    (src->value.s != NULL)) {
			dst->value.s = strdup(src->value.s);
		}
		src++;
		dst++;
	}

	return SLATEERROR_OK;
}

/**
 * frees an option table.
 *
 * Iterates through an option table a freeing resources as required
 * finally freeing the option table itself.
 *
 * @param opts The option table to free.
 */
static slateerror
slateoption_free(struct slateoption_s *opts)
{
	struct slateoption_s *cur; /* option being freed */

	if (opts == NULL) {
		return SLATEERROR_BAD_PARAMETER;
	}

	cur = opts;

	while (cur->key != NULL) {
		if ((cur->type == OPTION_STRING) && (cur->value.s != NULL)) {
			free(cur->value.s);
		}
		cur++;
	}
	free(opts);

	return SLATEERROR_OK;
}


/**
 * extract key/value from a line of input
 *
 * \retun SLATEERROR_OK and key_out and value_out updated
 *        SLATEERROR_NOT_FOUND if not a key/value input line
 *        SLATEERROR_INVALID if the line is and invalid format (missing colon)
 */
static slateerror
get_key_value(char *line, int linelen, char **key_out, char **value_out)
{
	char *key;
	char *value;

	/* skip leading whitespace for start of key */
	for (key = line; *key != 0; key++) {
		if ((*key != ' ') && (*key != '\t') && (*key != '\n')) {
			break;
		}
	}

	/* empty line or only whitespace */
	if (*key == 0) {
		return SLATEERROR_NOT_FOUND;
	}

	/* comment */
	if (*key == '#') {
		return SLATEERROR_NOT_FOUND;
	}

	/* get start of value */
	for (value = key; *value != 0; value++) {
		if (*value == ':') {
			*value = 0;
			value++;
			break;
		}
	}

	/* missing colon separator */
	if (*value == 0) {
		return SLATEERROR_INVALID;
	}

	/* remove delimiter from value */
	if (line[linelen - 1] == '\n') {
		linelen--;
		line[linelen] = 0;
	}

	*key_out = key;
	*value_out = value;
	return SLATEERROR_OK;
}


/**
 * Process a line from a user option file
 */
static slateerror optionline(struct slateoption_s *opts, char *line, int linelen)
{
	slateerror res;
	char *key;
	char *value;
	int idx;

	res = get_key_value(line, linelen, &key, &value);
	if (res != SLATEERROR_OK) {
		/* skip line as no valid key value pair found */
		return res;
	}

	for (idx = 0; opts[idx].key != NULL; idx++) {
		if (strcasecmp(key, opts[idx].key) == 0) {
			strtooption(value, &opts[idx]);
			break;
		}
	}

	return res;
}


/* exported interface documented in utils/slateoption.h */
slateerror
slateoption_init(slateoption_set_default_t *set_defaults,
	      struct slateoption_s **popts,
	      struct slateoption_s **pdefs)
{
	slateerror ret;
	struct slateoption_s *defs;
	struct slateoption_s *opts;

	ret = slateoption_dup(&defaults[0], &defs);
	if (ret != SLATEERROR_OK) {
		return ret;
	}

	/* update the default table */
	if (set_defaults != NULL) {
		/** @todo it would be better if the frontends actually
		 * set values in the passed in table instead of
		 * assuming the global one.
		 */
		opts = slateoptions;
		slateoptions = defs;

		ret = set_defaults(defs);

		if (ret != SLATEERROR_OK) {
			slateoptions = opts;
			slateoption_free(defs);
			return ret;
		}
	}

	/* copy the default values into the working set */
	ret = slateoption_dup(defs, &opts);
	if (ret != SLATEERROR_OK) {
		slateoption_free(defs);
		return ret;
	}

	/* return values if wanted */
	if (popts != NULL) {
		*popts = opts;
	} else {
		slateoptions = opts;
	}

	if (pdefs != NULL) {
		*pdefs = defs;
	} else {
		slateoptions_default = defs;
	}

	return SLATEERROR_OK;
}

/* exported interface documented in utils/slateoption.h */
slateerror slateoption_finalise(struct slateoption_s *opts, struct slateoption_s *defs)
{
	slateerror res;

	/* check to see if global table selected */
	if (opts == NULL) {
		res = slateoption_free(slateoptions);
		if (res == SLATEERROR_OK) {
			slateoptions = NULL;
		}
	} else {
		res = slateoption_free(opts);
	}
	if (res != SLATEERROR_OK) {
		return res;
	}

	/* check to see if global table selected */
	if (defs == NULL) {
		res = slateoption_free(slateoptions_default);
		if (res == SLATEERROR_OK) {
			slateoptions_default = NULL;
		}
	} else {
		res = slateoption_free(defs);
	}

	return res;
}


/* exported interface documented in utils/slateoption.h */
slateerror
slateoption_read(const char *path, struct slateoption_s *opts)
{
	char s[SLATEOPTION_MAX_LINE_LEN];
	FILE *fp;
	struct slateoption_s *defs;

	if (path == NULL) {
		return SLATEERROR_BAD_PARAMETER;
	}

	/* check to see if global table selected */
	if (opts == NULL) {
		opts = slateoptions;
	}

	/**
	 * @todo is this an API bug not being a parameter
	 */
	defs = slateoptions_default;

	if ((opts == NULL) || (defs == NULL)) {
		return SLATEERROR_BAD_PARAMETER;
	}

	fp = fopen(path, "r");
	if (!fp) {
		NSLOG(netsurf, INFO, "Failed to open file '%s'", path);
		return SLATEERROR_NOT_FOUND;
	}

	NSLOG(netsurf, INFO, "Successfully opened '%s' for Options file", path);

	while (fgets(s, SLATEOPTION_MAX_LINE_LEN, fp)) {
		optionline(opts, s, strlen(s));
	}

	fclose(fp);

	slateoption_validate(opts, defs);

	return SLATEERROR_OK;
}


/*
 * Generate options via callback.
 *
 * exported interface documented in utils/slateoption.h
 */
slateerror
slateoption_generate(slateoption_generate_cb *generate_cb,
		  void *generate_ctx,
		  enum slateoption_generate_flags flags,
		  struct slateoption_s *opts,
		  struct slateoption_s *defs)
{
	unsigned int entry; /* index to option being output */

	/* check to see if global table selected */
	if (opts == NULL) {
		opts = slateoptions;
	}

	/* check to see if global table selected */
	if (defs == NULL) {
		defs = slateoptions_default;
	}

	if ((opts == NULL) || (defs == NULL)) {
		return SLATEERROR_BAD_PARAMETER;
	}

	for (entry = 0; entry < SLATEOPTION_LISTEND; entry++) {
		if (((flags & SLATEOPTION_GENERATE_CHANGED) != 0) &&
		    (slateoption_is_set(opts, defs, entry) == false)) {
			continue;
		}
		generate_cb(opts + entry, generate_ctx);
	}

	return SLATEERROR_OK;
}


/*
 * Write options that have changed from the defaults to a file.
 *
 * exported interface documented in utils/slateoption.h
 */
slateerror
slateoption_write(const char *path,
	       struct slateoption_s *opts,
	       struct slateoption_s *defs)
{
	FILE *fp;
	slateerror ret;

	if (path == NULL) {
		return SLATEERROR_BAD_PARAMETER;
	}

	/* check to see if global table selected */
	if (opts == NULL) {
		opts = slateoptions;
	}

	/* check to see if global table selected */
	if (defs == NULL) {
		defs = slateoptions_default;
	}

	if ((opts == NULL) || (defs == NULL)) {
		return SLATEERROR_BAD_PARAMETER;
	}

	fp = fopen(path, "w");
	if (!fp) {
		NSLOG(netsurf, INFO, "failed to open file '%s' for writing",
		      path);
		return SLATEERROR_NOT_FOUND;
	}

	ret = slateoption_generate(slateoption_output_value_file,
				fp,
				SLATEOPTION_GENERATE_CHANGED,
				opts,
				defs);

	fclose(fp);

	return ret;
}


/* exported interface documented in utils/slateoption.h */
slateerror
slateoption_dump(FILE *outf, struct slateoption_s *opts)
{
	if (outf == NULL) {
		return SLATEERROR_BAD_PARAMETER;
	}

	/* check to see if global table selected and available */
	if (opts == NULL) {
		opts = slateoptions;
	}
	if (opts == NULL) {
		return SLATEERROR_BAD_PARAMETER;
	}

	return slateoption_generate(slateoption_output_value_file,
				outf,
				SLATEOPTION_GENERATE_ALL,
				opts,
				slateoptions_default);
}


/* exported interface documented in utils/slateoption.h */
slateerror
slateoption_commandline(int *pargc, char **argv, struct slateoption_s *opts)
{
	char *arg;
	char *val;
	int arglen;
	int idx = 1;
	int mv_loop;
	unsigned int entry_loop;

	if ((pargc == NULL) || (argv == NULL)) {
		return SLATEERROR_BAD_PARAMETER;
	}

	/* check to see if global table selected and available */
	if (opts == NULL) {
		opts = slateoptions;
	}
	if (opts == NULL) {
		return SLATEERROR_BAD_PARAMETER;
	}

	while (idx < *pargc) {
		arg = argv[idx];
		arglen = strlen(arg);

		/* check we have an option */
		/* option must start -- and be as long as the shortest option*/
		if ((arglen < (2+5) ) || (arg[0] != '-') || (arg[1] != '-'))
			break;

		arg += 2; /* skip -- */

		val = strchr(arg, '=');
		if (val == NULL) {
			/* no equals sign - next parameter is val */
			idx++;
			if (idx >= *pargc)
				break;
			val = argv[idx];
		} else {
			/* equals sign */
			arglen = val - arg ;
			val++;
		}

		/* arg+arglen is the option to set, val is the value */

		NSLOG(netsurf, INFO, "%.*s = %s", arglen, arg, val);

		for (entry_loop = 0;
		     entry_loop < SLATEOPTION_LISTEND;
		     entry_loop++) {
			if (strncmp(arg, opts[entry_loop].key, arglen) == 0) {
				strtooption(val, opts + entry_loop);
				break;
			}
		}

		idx++;
	}

	/* remove processed options from argv */
	for (mv_loop=0; mv_loop < (*pargc - idx); mv_loop++) {
		argv[mv_loop + 1] = argv[mv_loop + idx];
	}
	*pargc -= (idx - 1);

	slateoption_validate(opts, slateoptions_default);

	return SLATEERROR_OK;
}

/* exported interface documented in options.h */
int
slateoption_snoptionf(char *string,
		   size_t size,
		   enum slateoption_e option_idx,
		   const char *fmt)
{
	size_t slen = 0; /* current output string length */
	int fmtc = 0; /* current index into format string */
	struct slateoption_s *option;

	if (fmt == NULL) {
		return -1;
	}

	if (option_idx >= SLATEOPTION_LISTEND) {
		return -1;
	}

	if (slateoptions == NULL) {
		return -1;
	}

	option = &slateoptions[option_idx]; /* assume the global table */
	if (option == NULL || option->key == NULL) {
		return -1;
	}

	while ((slen < size) && (fmt[fmtc] != 0)) {
		if (fmt[fmtc] == '%') {
			fmtc++;
			switch (fmt[fmtc]) {
			case 'k':
				slen += snprintf(string + slen,
						 size - slen,
						 "%s",
						 option->key);
				break;

			case 'p':
				if (slateoption_is_set(slateoptions,
						    slateoptions_default,
						    option_idx)) {
					slen += snprintf(string + slen,
							 size - slen,
							 "user");
				} else {
					slen += snprintf(string + slen,
							 size - slen,
							 "default");
				}
				break;

			case 't':
				switch (option->type) {
				case OPTION_BOOL:
					slen += snprintf(string + slen,
							 size - slen,
							 "boolean");
					break;

				case OPTION_INTEGER:
					slen += snprintf(string + slen,
							 size - slen,
							 "integer");
					break;

				case OPTION_UINT:
					slen += snprintf(string + slen,
							 size - slen,
							 "unsigned integer");
					break;

				case OPTION_COLOUR:
					slen += snprintf(string + slen,
							 size - slen,
							 "colour");
					break;

				case OPTION_STRING:
					slen += snprintf(string + slen,
							 size - slen,
							 "string");
					break;

				}
				break;


			case 'V':
				slen += slateoption_output_value_html(option,
								   size,
								   slen,
								   string);
				break;
			case 'v':
				slen += slateoption_output_value_text(option,
								   size,
								   slen,
								   string);
				break;
			}
			fmtc++;
		} else {
			string[slen] = fmt[fmtc];
			slen++;
			fmtc++;
		}
	}

	/* Ensure that we NUL-terminate the output */
	string[min(slen, size - 1)] = '\0';

	return slen;
}

/* exported interface documented in options.h */
slateerror
slateoption_set_tbl_charp(struct slateoption_s *opts,
		       enum slateoption_e option_idx,
		       char *s)
{
	struct slateoption_s *option;

	option = &opts[option_idx];

	/* ensure it is a string option */
	if (option->type != OPTION_STRING) {
		return SLATEERROR_BAD_PARAMETER;
	}

	/* free any existing string */
	if (option->value.s != NULL) {
		free(option->value.s);
	}

	option->value.s = s;

	/* check for empty string */
	if ((option->value.s != NULL) && (*option->value.s == 0)) {
		free(option->value.s);
		option->value.s = NULL;
	}
	return SLATEERROR_OK;
}
