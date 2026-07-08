/*
 * Copyright 2012 Michael Drake <tlsa@netsurf-browser.org>
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
 * Useful interned string tables implementation.
 */

#include <dom/dom.h>

#include "utils/corestrings.h"
#include "utils/slateurl.h"
#include "utils/utils.h"

/* define corestrings */
#define CORESTRING_LWC_VALUE(NAME,VALUE) lwc_string *corestring_lwc_##NAME
#define CORESTRING_DOM_VALUE(NAME,VALUE) dom_string *corestring_dom_##NAME
#define CORESTRING_SLATEURL(NAME,VALUE) slateurl *corestring_slateurl_##NAME
#include "utils/corestringlist.h"
#undef CORESTRING_LWC_VALUE
#undef CORESTRING_DOM_VALUE
#undef CORESTRING_SLATEURL

/* exported interface documented in utils/corestrings.h */
slateerror corestrings_fini(void)
{
#define CORESTRING_LWC_VALUE(NAME,VALUE)				\
	do {								\
		lwc_string_unref(corestring_lwc_##NAME);		\
		corestring_lwc_##NAME = NULL;				\
	} while (0)

#define CORESTRING_DOM_VALUE(NAME,VALUE)				\
	do {								\
		if (corestring_dom_##NAME != NULL) {			\
			dom_string_unref(corestring_dom_##NAME);	\
			corestring_dom_##NAME = NULL;			\
		}							\
	} while (0)

#define CORESTRING_SLATEURL(NAME,VALUE)					\
	do {								\
		if (corestring_slateurl_##NAME != NULL) {			\
			slateurl_unref(corestring_slateurl_##NAME);		\
			corestring_slateurl_##NAME = NULL;			\
		}							\
	} while (0)


#include "utils/corestringlist.h"

#undef CORESTRING_LWC_VALUE
#undef CORESTRING_DOM_VALUE
#undef CORESTRING_SLATEURL

	return SLATEERROR_OK;
}


/* exported interface documented in utils/corestrings.h */
slateerror corestrings_init(void)
{
	lwc_error lerror;
	slateerror error;
	dom_exception exc;

#define CORESTRING_LWC_VALUE(NAME,VALUE)				\
	do {								\
		lerror = lwc_intern_string(				\
			(const char *)VALUE,				\
			sizeof(VALUE) - 1,				\
			&corestring_lwc_##NAME );			\
		if ((lerror != lwc_error_ok) ||				\
		    (corestring_lwc_##NAME == NULL)) {			\
			error = SLATEERROR_NOMEM;				\
			goto error;					\
		}							\
	} while(0)

#define CORESTRING_DOM_VALUE(NAME,VALUE)				\
	do {								\
		exc = dom_string_create_interned(			\
				(const uint8_t *)VALUE,			\
				sizeof(VALUE) - 1,			\
				&corestring_dom_##NAME );		\
		if ((exc != DOM_NO_ERR) ||				\
				(corestring_dom_##NAME == NULL)) {	\
			error = SLATEERROR_NOMEM;				\
			goto error;					\
		}							\
	} while(0)

#define CORESTRING_SLATEURL(NAME,VALUE)					\
	do {								\
		error = slateurl_create(VALUE,				\
				     &corestring_slateurl_##NAME);		\
		if (error != SLATEERROR_OK) {				\
			goto error;					\
		}							\
	} while(0)

#include "utils/corestringlist.h"

#undef CORESTRING_LWC_VALUE
#undef CORESTRING_DOM_VALUE
#undef CORESTRING_SLATEURL

	return SLATEERROR_OK;

error:
	corestrings_fini();

	return error;
}
