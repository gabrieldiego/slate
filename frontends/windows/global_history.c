/*
 * Copyright 2016 Vincent Sanders <vince@netsurf-browser.org>
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
 * Implementation of win32 global history interface.
 */

#include <stdint.h>
#include <stdlib.h>
#include <windows.h>

#include "utils/log.h"
#include "utils/slateoption.h"
#include "slate/keypress.h"
#include "slate/plotters.h"
#include "desktop/global_history.h"

#include "windows/plot.h"
#include "windows/corewindow.h"
#include "windows/global_history.h"


struct slatew32_global_history_window {
	struct slatew32_corewindow core;
};

static struct slatew32_global_history_window *global_history_window = NULL;

/**
 * callback for keypress on global_history window
 *
 * \param slatew32_cw The slatew32 core window structure.
 * \param nskey The netsurf key code
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slatew32_global_history_key(struct slatew32_corewindow *slatew32_cw, uint32_t nskey)
{
	if (global_history_keypress(nskey)) {
		return SLATEERROR_OK;
	}
	return SLATEERROR_NOT_IMPLEMENTED;
}

/**
 * callback for mouse action on global_history window
 *
 * \param slatew32_cw The slatew32 core window structure.
 * \param mouse_state netsurf mouse state on event
 * \param x location of event
 * \param y location of event
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slatew32_global_history_mouse(struct slatew32_corewindow *slatew32_cw,
		    browser_mouse_state mouse_state,
		    int x, int y)
{
	global_history_mouse_action(mouse_state, x, y);

	return SLATEERROR_OK;
}

/**
 * callback on draw event for global_history window
 *
 * \param slatew32_cw The slatew32 core window structure.
 * \param scrollx The horizontal scroll offset.
 * \param scrolly The vertical scroll offset.
 * \param r The rectangle of the window that needs updating.
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slatew32_global_history_draw(struct slatew32_corewindow *slatew32_cw,
			  int scrollx,
			  int scrolly,
			  struct rect *r)
{
	struct redraw_context ctx = {
		.interactive = true,
		.background_images = true,
		.plot = &win_plotters
	};

	global_history_redraw(-scrollx, -scrolly, r, &ctx);

	return SLATEERROR_OK;
}


static slateerror
slatew32_global_history_close(struct slatew32_corewindow *slatew32_cw)
{
	ShowWindow(slatew32_cw->hWnd, SW_HIDE);

	return SLATEERROR_OK;
}

/**
 * Creates the window for the global_history tree.
 *
 * \return SLATEERROR_OK on success else appropriate error code on faliure.
 */
static slateerror slatew32_global_history_init(HINSTANCE hInstance)
{
	struct slatew32_global_history_window *ncwin;
	slateerror res;

	if (global_history_window != NULL) {
		return SLATEERROR_OK;
	}

	ncwin = calloc(1, sizeof(*ncwin));
	if (ncwin == NULL) {
		return SLATEERROR_NOMEM;
	}

	ncwin->core.title = "NetSurf Global History";
	ncwin->core.draw = slatew32_global_history_draw;
	ncwin->core.key = slatew32_global_history_key;
	ncwin->core.mouse = slatew32_global_history_mouse;
	ncwin->core.close = slatew32_global_history_close;

	res = slatew32_corewindow_init(hInstance, NULL, &ncwin->core);
	if (res != SLATEERROR_OK) {
		free(ncwin);
		return res;
	}

	res = global_history_init((struct core_window *)ncwin);
	if (res != SLATEERROR_OK) {
		free(ncwin);
		return res;
	}

	/* memoise window so it can be represented when necessary
	 * instead of recreating every time.
	 */
	global_history_window = ncwin;

	return SLATEERROR_OK;
}


/* exported interface documented in windows/global_history.h */
slateerror slatew32_global_history_present(HINSTANCE hInstance)
{
	slateerror res;

	res = slatew32_global_history_init(hInstance);
	if (res == SLATEERROR_OK) {
		ShowWindow(global_history_window->core.hWnd, SW_SHOWNORMAL);
	}
	return res;
}

/* exported interface documented in windows/global_history.h */
slateerror slatew32_global_history_finalise(void)
{
	slateerror res;

	if (global_history_window == NULL) {
		return SLATEERROR_OK;
	}

	res = global_history_fini();
	if (res == SLATEERROR_OK) {
		res = slatew32_corewindow_fini(&global_history_window->core);
		DestroyWindow(global_history_window->core.hWnd);
		free(global_history_window);
		global_history_window = NULL;
	}

	return res;
}
