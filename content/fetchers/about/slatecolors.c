/*
 * Copyright 2020 Vincent Sanders <vince@netsurf-browser.org>
 *
 * This file is part of NetSurf.
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
 * content generator for the about scheme query privacy page
 */

#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include <stdlib.h>

#include "slate/plot_style.h"

#include "utils/errors.h"
#include "utils/slatecolor.h"

#include "private.h"
#include "slatecolors.h"

/**
 * Handler to generate the slatecolors stylesheet
 *
 * \param ctx The fetcher context.
 * \return true if handled false if aborted.
 */
bool fetch_about_slatecolors_handler(struct fetch_about_context *ctx)
{
	slateerror res;
	const char *stylesheet;

	/* content is going to return ok */
	fetch_about_set_http_code(ctx, 200);

	/* content type */
	if (fetch_about_send_header(ctx, "Content-Type: text/css; charset=utf-8")) {
		goto aborted;
	}

	res = slatecolor_get_stylesheet(&stylesheet);
	if (res != SLATEERROR_OK) {
		goto aborted;
	}

	res = fetch_about_ssenddataf(ctx,
			"html {\n"
			"\tbackground-color: #%06x;\n"
			"}\n"
			"%s",
			colour_rb_swap(slatecolors[SLATECOLOR_WIN_ODD_BG]),
			stylesheet);
	if (res != SLATEERROR_OK) {
		goto aborted;
	}

	fetch_about_send_finished(ctx);

	return true;

aborted:

	return false;
}
