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
 * Option reading and saving interface.
 *
 * Global options are defined in desktop/options.h
 * Distinct target options are defined in ${TARGET}/options.h
 *
 * The implementation API is slightly compromised because it still has
 * "global" tables for both the default and current option tables.
 *
 * The initialisation and read/write interfaces take pointers to an
 * option table which would let us to make the option structure
 * opaque.
 *
 * All the actual acessors assume direct access to a global option
 * table (slateoptions). To avoid this the acessors would have to take a
 * pointer to the active options table and be implemented as functions
 * within slateoptions.c
 *
 * Indirect access would have an impact on performance of NetSurf as
 * the expected option lookup cost is currently that of a simple
 * dereference (which this current implementation keeps).
 */

#ifndef _SLATE_UTILS_SLATEOPTION_H_
#define _SLATE_UTILS_SLATEOPTION_H_

#include <stdio.h>
#include <stdint.h>
#include <stdbool.h>

#include "utils/errors.h"

/* allow targets to include any necessary headers of their own */
#define SLATEOPTION_BOOL(NAME, DEFAULT)
#define SLATEOPTION_STRING(NAME, DEFAULT)
#define SLATEOPTION_INTEGER(NAME, DEFAULT)
#define SLATEOPTION_UINT(NAME, DEFAULT)
#define SLATEOPTION_COLOUR(NAME, DEFAULT)

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

#undef SLATEOPTION_BOOL
#undef SLATEOPTION_STRING
#undef SLATEOPTION_INTEGER
#undef SLATEOPTION_UINT
#undef SLATEOPTION_COLOUR



enum { OPTION_HTTP_PROXY_AUTH_NONE = 0,
       OPTION_HTTP_PROXY_AUTH_BASIC = 1,
       OPTION_HTTP_PROXY_AUTH_NTLM = 2 };

#define DEFAULT_MARGIN_TOP_MM 10
#define DEFAULT_MARGIN_BOTTOM_MM 10
#define DEFAULT_MARGIN_LEFT_MM 10
#define DEFAULT_MARGIN_RIGHT_MM 10
#define DEFAULT_EXPORT_SCALE 0.7

#ifndef DEFAULT_REFLOW_PERIOD
/** Default reflow time in cs */
#define DEFAULT_REFLOW_PERIOD 25
#endif

/** The options type. */
enum slateoption_type_e {
	OPTION_BOOL, /**< Option is a boolean. */
	OPTION_INTEGER, /**< Option is an integer. */
	OPTION_UINT, /**< Option is an unsigned integer */
	OPTION_STRING, /**< option is a heap allocated string. */
	OPTION_COLOUR /**< Option  is a netsurf colour. */
};

struct slateoption_s {
	const char *key;
	int key_len;
	enum slateoption_type_e type;
	union {
		bool b;
		int i;
		unsigned int u;
		char *s;
		const char *cs;
		colour c;
	} value;
};

/* construct the option enumeration */
#define SLATEOPTION_BOOL(NAME, DEFAULT) SLATEOPTION_##NAME,
#define SLATEOPTION_STRING(NAME, DEFAULT) SLATEOPTION_##NAME,
#define SLATEOPTION_INTEGER(NAME, DEFAULT) SLATEOPTION_##NAME,
#define SLATEOPTION_UINT(NAME, DEFAULT) SLATEOPTION_##NAME,
#define SLATEOPTION_COLOUR(NAME, DEFAULT) SLATEOPTION_##NAME,

enum slateoption_e {
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
	SLATEOPTION_LISTEND /* end of list */
};

#undef SLATEOPTION_BOOL
#undef SLATEOPTION_STRING
#undef SLATEOPTION_INTEGER
#undef SLATEOPTION_UINT
#undef SLATEOPTION_COLOUR

/**
 * global active option table.
 */
extern struct slateoption_s *slateoptions;


/**
 * global default option table.
 */
extern struct slateoption_s *slateoptions_default;


/**
 * default setting callback.
 */
typedef slateerror(slateoption_set_default_t)(struct slateoption_s *defaults);


/**
 * option generate callback
 */
typedef size_t(slateoption_generate_cb)(struct slateoption_s *option, void *ctx);


/**
 * flags to control option output in the generate call
 */
enum slateoption_generate_flags {
	/** Generate output for all options */
	SLATEOPTION_GENERATE_ALL = 0,
	/** Generate output for options which differ from the default */
	SLATEOPTION_GENERATE_CHANGED = 1,
};


/**
 * Initialise option system.
 *
 * @param set_default callback to allow the customisation of the default
 *                    options.
 * @param popts pointer to update to get options table or NULL.
 * @param pdefs pointer to update to get default options table or NULL.
 * @return The error status
 */
slateerror slateoption_init(slateoption_set_default_t *set_default, struct slateoption_s **popts, struct slateoption_s **pdefs);


/**
 * Finalise option system
 *
 * Releases all resources allocated in the initialisation.
 *
 * @param opts the options table or NULL to use global table.
 * @param defs the default options table to use or NULL to use global table
 * return The error status
 */
slateerror slateoption_finalise(struct slateoption_s *opts, struct slateoption_s *defs);


/**
 * Read choices file and set them in the passed table
 *
 * @param path The path to read the file from
 * @param opts The options table to enerate values from or NULL to use global
 * @return The error status
 */
slateerror slateoption_read(const char *path, struct slateoption_s *opts);


/**
 * Generate options via acallback.
 *
 * iterates options controlled by flags calling a method for each matched option.
 *
 * @param cb Function called for each option to be output.
 * @param ctx The context for the callback.
 * @param flags Flags controlling option matching.
 * @param opts The options table to enerate values from or NULL to use global.
 * @param defs The default table to use or NULL to use global.
 * @return The error status.
 */
slateerror slateoption_generate(slateoption_generate_cb *cb, void *ctx, enum slateoption_generate_flags flags, struct slateoption_s *opts, struct slateoption_s *defs);


/**
 * Write options that have changed from the defaults to a file.
 *
 * The \a slateoption_dump can be used to output all entries not just
 * changed ones.
 *
 * @param path The path to read the file from
 * @param opts The options table to enerate values from or NULL to use global
 * @param defs The default table to use or NULL to use global
 * @return The error status
 */
slateerror slateoption_write(const char *path, struct slateoption_s *opts, struct slateoption_s *defs);


/**
 * Write all options to a stream.
 *
 * @param outf The stream to write to
 * @param opts The options table to enerate values from or NULL to use global
 * @return The error status
 */
slateerror slateoption_dump(FILE *outf, struct slateoption_s *opts);


/**
 * Process commandline and set options approriately.
 *
 * @param pargc Pointer to the size of the argument vector.
 * @param argv The argument vector.
 * @param opts The options table to enerate values from or NULL to use global
 * @return The error status
 */
slateerror slateoption_commandline(int *pargc, char **argv, struct slateoption_s *opts);


/**
 * Fill a buffer with an option using a format.
 *
 * The format string is copied into the output buffer with the
 * following replaced:
 * %k - The options key
 * %t - The options type
 * %V - value (HTML formatting)
 * %v - value (plain formatting)
 * %p - provenance either "user" or "default"
 *
 * @param string The buffer in which to place the results.
 * @param size The size of the string buffer.
 * @param option The option .
 * @param fmt The format string.
 * @return The number of bytes written to \a string or -1 on error
 */
int slateoption_snoptionf(char *string, size_t size, enum slateoption_e option, const char *fmt);


/**
 * Get the value of a boolean option.
 *
 * Gets the value of an option assuming it is a boolean type.
 * @note option type is unchecked so care must be taken in caller.
 */
#define slateoption_bool(OPTION) (slateoptions[SLATEOPTION_##OPTION].value.b)


/**
 * Get the value of an integer option.
 *
 * Gets the value of an option assuming it is a integer type.
 * @note option type is unchecked so care must be taken in caller.
 */
#define slateoption_int(OPTION) (slateoptions[SLATEOPTION_##OPTION].value.i)


/**
 * Get the value of an unsigned integer option.
 *
 * Gets the value of an option assuming it is a integer type.
 * @note option type is unchecked so care must be taken in caller.
 */
#define slateoption_uint(OPTION) (slateoptions[SLATEOPTION_##OPTION].value.u)


/**
 * Get the value of a string option.
 *
 * Gets the value of an option assuming it is a string type.
 * @note option type is unchecked so care must be taken in caller.
 */
#define slateoption_charp(OPTION) (slateoptions[SLATEOPTION_##OPTION].value.s)


/**
 * Get the value of a netsurf colour option.
 *
 * Gets the value of an option assuming it is a colour type.
 * @note option type is unchecked so care must be taken in caller.
 */
#define slateoption_colour(OPTION) (slateoptions[SLATEOPTION_##OPTION].value.c)


/** set a boolean option in the default table */
#define slateoption_set_bool(OPTION, VALUE) slateoptions[SLATEOPTION_##OPTION].value.b = VALUE


/** set an integer option in the default table */
#define slateoption_set_int(OPTION, VALUE) slateoptions[SLATEOPTION_##OPTION].value.i = VALUE

/** set an unsigned integer option in the default table */
#define slateoption_set_uint(OPTION, VALUE) slateoptions[SLATEOPTION_##OPTION].value.u = VALUE


/** set a colour option in the default table */
#define slateoption_set_colour(OPTION, VALUE) slateoptions[SLATEOPTION_##OPTION].value.c = VALUE


/**
 * Set string option in specified table.
 *
 * Sets the string option to the value given freeing any resources
 * currently allocated to the option. If the passed string is empty it
 * is converted to the NULL value.
 *
 * @param opts The table to set option in
 * @param option_idx The option
 * @param s The string to set. This is used directly and not copied.
 */
slateerror slateoption_set_tbl_charp(struct slateoption_s *opts, enum slateoption_e option_idx, char *s);

/** set string option in default table */
#define slateoption_set_charp(OPTION, VALUE) \
	slateoption_set_tbl_charp(slateoptions, SLATEOPTION_##OPTION, VALUE)

/** set string option in default table if currently unset */
#define slateoption_setnull_charp(OPTION, VALUE)				\
	do {								\
		if (slateoptions[SLATEOPTION_##OPTION].value.s == NULL) {	\
			slateoption_set_tbl_charp(slateoptions, SLATEOPTION_##OPTION, VALUE); \
		} else {						\
			free(VALUE);					\
		}							\
	} while (0)

#endif
