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
 * Implementation of win32 bookmark (hotlist) manager.
 */

#include <stdint.h>
#include <stdlib.h>
#include <windows.h>

#include "utils/log.h"
#include "utils/slateoption.h"
#include "slate/keypress.h"
#include "slate/plotters.h"
#include "desktop/hotlist.h"

#include "windows/plot.h"
#include "windows/corewindow.h"
#include "windows/hotlist.h"


/**
 * Hotlist window container for win32.
 */
struct slatew32_hotlist_window {
	struct slatew32_corewindow core;
};

/** hotlist window singleton */
static struct slatew32_hotlist_window *hotlist_window = NULL;

/**
 * callback for keypress on hotlist window
 *
 * \param slatew32_cw The slatew32 core window structure.
 * \param nskey The netsurf key code
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slatew32_hotlist_key(struct slatew32_corewindow *slatew32_cw, uint32_t nskey)
{
	if (hotlist_keypress(nskey)) {
		return SLATEERROR_OK;
	}
	return SLATEERROR_NOT_IMPLEMENTED;
}

/**
 * callback for mouse action on hotlist window
 *
 * \param slatew32_cw The slatew32 core window structure.
 * \param mouse_state netsurf mouse state on event
 * \param x location of event
 * \param y location of event
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slatew32_hotlist_mouse(struct slatew32_corewindow *slatew32_cw,
		    browser_mouse_state mouse_state,
		    int x, int y)
{
	hotlist_mouse_action(mouse_state, x, y);

	return SLATEERROR_OK;
}

/**
 * callback on draw event for hotlist window
 *
 * \param slatew32_cw The slatew32 core window structure.
 * \param scrollx The horizontal scroll offset.
 * \param scrolly The vertical scroll offset.
 * \param r The rectangle of the window that needs updating.
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slatew32_hotlist_draw(struct slatew32_corewindow *slatew32_cw,
		   int scrollx,
		   int scrolly,
		   struct rect *r)
{
	struct redraw_context ctx = {
		.interactive = true,
		.background_images = true,
		.plot = &win_plotters
	};

	hotlist_redraw(-scrollx, -scrolly, r, &ctx);

	return SLATEERROR_OK;
}


static slateerror
slatew32_hotlist_close(struct slatew32_corewindow *slatew32_cw)
{
	ShowWindow(slatew32_cw->hWnd, SW_HIDE);

	return SLATEERROR_OK;
}

/**
 * Creates the window for the hotlist tree.
 *
 * \return SLATEERROR_OK on success else appropriate error code on faliure.
 */
static slateerror slatew32_hotlist_init(HINSTANCE hInstance)
{
	struct slatew32_hotlist_window *ncwin;
	slateerror res;

	if (hotlist_window != NULL) {
		return SLATEERROR_OK;
	}

	ncwin = calloc(1, sizeof(*ncwin));
	if (ncwin == NULL) {
		return SLATEERROR_NOMEM;
	}

	ncwin->core.title = "NetSurf Bookmarks";
	ncwin->core.draw = slatew32_hotlist_draw;
	ncwin->core.key = slatew32_hotlist_key;
	ncwin->core.mouse = slatew32_hotlist_mouse;
	ncwin->core.close = slatew32_hotlist_close;
	
	res = slatew32_corewindow_init(hInstance, NULL, &ncwin->core);
	if (res != SLATEERROR_OK) {
		free(ncwin);
		return res;
	}

	res = hotlist_manager_init((struct core_window *)ncwin);
	if (res != SLATEERROR_OK) {
		free(ncwin);
		return res;
	}

	/* memoise window so it can be represented when necessary
	 * instead of recreating every time.
	 */
	hotlist_window = ncwin;
	
	return SLATEERROR_OK;
}


/* exported interface documented in windows/hotlist.h */
slateerror slatew32_hotlist_present(HINSTANCE hInstance)
{
	slateerror res;

	res = slatew32_hotlist_init(hInstance);
	if (res == SLATEERROR_OK) {
		ShowWindow(hotlist_window->core.hWnd, SW_SHOWNORMAL);
	}
	return res;
}

/* exported interface documented in windows/hotlist.h */
slateerror slatew32_hotlist_finalise(void)
{
	slateerror res;

	if (hotlist_window == NULL) {
		return SLATEERROR_OK;
	}

	res = hotlist_fini();
	if (res == SLATEERROR_OK) {
		res = slatew32_corewindow_fini(&hotlist_window->core);
		DestroyWindow(hotlist_window->core.hWnd);
		free(hotlist_window);
		hotlist_window = NULL;
	}

	return res;
}
