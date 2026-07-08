/*
 * Copyright 2020 Michael Drake <tlsa@netsurf-browser.org>
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

/** \file
 * NetSurf UI colours (interface).
 *
 * Interface to acquire common colours used throughout NetSurf's interface.
 */

#ifndef _SLATE_UTILS_SLATECOLOR_H_
#define _SLATE_UTILS_SLATECOLOR_H_

#include "slate/types.h"

/**
 * NetSurf UI colour key.
 */
enum slatecolor {
	SLATECOLOR_WIN_ODD_BG,
	SLATECOLOR_WIN_ODD_BG_HOVER,
	SLATECOLOR_WIN_ODD_FG,
	SLATECOLOR_WIN_ODD_FG_SUBTLE,
	SLATECOLOR_WIN_ODD_FG_FADED,
	SLATECOLOR_WIN_ODD_FG_GOOD,
	SLATECOLOR_WIN_ODD_FG_BAD,
	SLATECOLOR_WIN_ODD_BORDER,
	SLATECOLOR_WIN_EVEN_BG,
	SLATECOLOR_WIN_EVEN_BG_HOVER,
	SLATECOLOR_WIN_EVEN_FG,
	SLATECOLOR_WIN_EVEN_FG_SUBTLE,
	SLATECOLOR_WIN_EVEN_FG_FADED,
	SLATECOLOR_WIN_EVEN_FG_GOOD,
	SLATECOLOR_WIN_EVEN_FG_BAD,
	SLATECOLOR_WIN_EVEN_BORDER,
	SLATECOLOR_TEXT_INPUT_BG,
	SLATECOLOR_TEXT_INPUT_FG,
	SLATECOLOR_TEXT_INPUT_FG_SUBTLE,
	SLATECOLOR_SEL_BG,
	SLATECOLOR_SEL_FG,
	SLATECOLOR_SEL_FG_SUBTLE,
	SLATECOLOR_SCROLL_WELL,
	SLATECOLOR_BUTTON_BG,
	SLATECOLOR_BUTTON_FG,
	SLATECOLOR__COUNT,
};

/**
 * NetSurf UI colour table.
 */
extern colour slatecolors[];

/**
 * Update the slatecolor table from the current slateoptions.
 *
 * \return SLATEERROR_OK on success, or appropriate error otherwise.
 */
slateerror slatecolor_update(void);

/**
 * Get a pointer to a stylesheet for slatecolors.
 *
 * \return SLATEERROR_OK on success, or appropriate error otherwise.
 */
slateerror slatecolor_get_stylesheet(const char **stylesheet_out);

#endif
