/*
 * Copyright 2009 John-Mark Bell <jmb@netsurf-browser.org>
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

#include <string.h>
#include <libcss/libcss.h>

#include "utils/slateurl.h"

#include "css/internal.h"

/* exported interface documented in content/handlers/css/internal.h */
css_error nscss_resolve_url(void *pw, const char *base,
		lwc_string *rel, lwc_string **abs)
{
	lwc_error lerror;
	slateerror error;
	slateurl *nsbase;
	slateurl *nsabs;

	/* Create slateurl from base */
	/* TODO: avoid this */
	error = slateurl_create(base, &nsbase);
	if (error != SLATEERROR_OK) {
		return error == SLATEERROR_NOMEM ? CSS_NOMEM : CSS_INVALID;
	}

	/* Resolve URI */
	error = slateurl_join(nsbase, lwc_string_data(rel), &nsabs);
	if (error != SLATEERROR_OK) {
		slateurl_unref(nsbase);
		return error == SLATEERROR_NOMEM ? CSS_NOMEM : CSS_INVALID;
	}

	slateurl_unref(nsbase);

	/* Intern it */
	lerror = lwc_intern_string(slateurl_access(nsabs),
			slateurl_length(nsabs), abs);
	if (lerror != lwc_error_ok) {
		*abs = NULL;
		slateurl_unref(nsabs);
		return lerror == lwc_error_oom ? CSS_NOMEM : CSS_INVALID;
	}

	slateurl_unref(nsabs);

	return CSS_OK;
}
