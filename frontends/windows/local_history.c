/*
 * Copyright 2017 Vincent Sanders <vince@netsurf-browser.org>
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
 * Implementation of win32 local history interface.
 */

#include <stdint.h>
#include <stdlib.h>
#include <windows.h>

#include "utils/log.h"
#include "utils/slateoption.h"
#include "slate/keypress.h"
#include "slate/plotters.h"
#include "desktop/local_history.h"

#include "windows/plot.h"
#include "windows/corewindow.h"
#include "windows/local_history.h"


struct slatew32_local_history_window {
	struct slatew32_corewindow core;

	struct local_history_session *session;
};

static struct slatew32_local_history_window *local_history_window = NULL;

/**
 * callback for keypress on local_history window
 *
 * \param slatew32_cw The slatew32 core window structure.
 * \param nskey The netsurf key code
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slatew32_local_history_key(struct slatew32_corewindow *slatew32_cw, uint32_t nskey)
{
	struct slatew32_local_history_window *lhw;

	lhw = (struct slatew32_local_history_window *)slatew32_cw;

	if (local_history_keypress(lhw->session,nskey)) {
		return SLATEERROR_OK;
	}
	return SLATEERROR_NOT_IMPLEMENTED;
}

/**
 * callback for mouse action on local_history window
 *
 * \param slatew32_cw The slatew32 core window structure.
 * \param mouse_state netsurf mouse state on event
 * \param x location of event
 * \param y location of event
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slatew32_local_history_mouse(struct slatew32_corewindow *slatew32_cw,
		    browser_mouse_state mouse_state,
		    int x, int y)
{
	struct slatew32_local_history_window *lhw;

	lhw = (struct slatew32_local_history_window *)slatew32_cw;

	local_history_mouse_action(lhw->session, mouse_state, x, y);

	return SLATEERROR_OK;
}

/**
 * callback on draw event for local_history window
 *
 * \param slatew32_cw The slatew32 core window structure.
 * \param scrollx The horizontal scroll offset.
 * \param scrolly The vertical scroll offset.
 * \param r The rectangle of the window that needs updating.
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slatew32_local_history_draw(struct slatew32_corewindow *slatew32_cw,
			  int scrollx,
			  int scrolly,
			  struct rect *r)
{
	struct slatew32_local_history_window *lhw;
	struct redraw_context ctx = {
		.interactive = true,
		.background_images = true,
		.plot = &win_plotters
	};

	lhw = (struct slatew32_local_history_window *)slatew32_cw;

	local_history_redraw(lhw->session, -scrollx, -scrolly, r, &ctx);

	return SLATEERROR_OK;
}


static slateerror
slatew32_local_history_close(struct slatew32_corewindow *slatew32_cw)
{
	ShowWindow(slatew32_cw->hWnd, SW_HIDE);

	return SLATEERROR_OK;
}

/**
 * Creates the window for the local_history tree.
 *
 * \return SLATEERROR_OK on success else appropriate error code on faliure.
 */
static slateerror
slatew32_local_history_init(HINSTANCE hInstance,
			 struct browser_window *bw,
			 struct slatew32_local_history_window **win_out)
{
	struct slatew32_local_history_window *ncwin;
	slateerror res;

	if ((*win_out) != NULL) {
		res = local_history_set((*win_out)->session, bw);
		return res;
	}

	ncwin = calloc(1, sizeof(*ncwin));
	if (ncwin == NULL) {
		return SLATEERROR_NOMEM;
	}

	ncwin->core.title = "NetSurf Local History";
	ncwin->core.draw = slatew32_local_history_draw;
	ncwin->core.key = slatew32_local_history_key;
	ncwin->core.mouse = slatew32_local_history_mouse;
	ncwin->core.close = slatew32_local_history_close;

	res = slatew32_corewindow_init(hInstance, NULL, &ncwin->core);
	if (res != SLATEERROR_OK) {
		free(ncwin);
		return res;
	}

	res = local_history_init((struct core_window *)ncwin,
				 bw,
				 &ncwin->session);
	if (res != SLATEERROR_OK) {
		free(ncwin);
		return res;
	}

	/* memoise window so it can be represented when necessary
	 * instead of recreating every time.
	 */
	*win_out = ncwin;

	return SLATEERROR_OK;
}


/* exported interface documented in windows/local_history.h */
slateerror
slatew32_local_history_present(HWND hWndParent, struct browser_window *bw)
{
	slateerror res;
	HINSTANCE hInstance;
	RECT parentr;
	int width, height;
	int margin = 50;

	hInstance = (HINSTANCE)GetWindowLongPtr(hWndParent, GWLP_HINSTANCE);

	res = slatew32_local_history_init(hInstance, bw, &local_history_window);
	if (res == SLATEERROR_OK) {
		GetWindowRect(hWndParent, &parentr);

		/* resize history widget ensureing the drawing area is
		 * no larger than parent window
		 */
		res = local_history_get_size(local_history_window->session,
					     &width,
					     &height);
		width += margin;
		height += margin;
		if ((parentr.right - parentr.left - margin) < width) {
			width = parentr.right - parentr.left - margin;
		}
		if ((parentr.bottom - parentr.top - margin) < height) {
			height = parentr.bottom - parentr.top - margin;
		}
		SetWindowPos(local_history_window->core.hWnd,
			     HWND_TOP,
			     parentr.left + (margin/2),
			     parentr.top + (margin/2),
			     width,
			     height,
			     SWP_SHOWWINDOW);
		local_history_scroll_to_cursor(local_history_window->session);
	}
	return res;
}


/* exported interface documented in windows/local_history.h */
slateerror slatew32_local_history_hide(void)
{
	slateerror res = SLATEERROR_OK;

	if (local_history_window != NULL) {
		ShowWindow(local_history_window->core.hWnd, SW_HIDE);

		res = local_history_set(local_history_window->session, NULL);
	}

	return res;
}

/* exported interface documented in windows/local_history.h */
slateerror slatew32_local_history_finalise(void)
{
	slateerror res;

	if (local_history_window == NULL) {
		return SLATEERROR_OK;
	}

	res = local_history_fini(local_history_window->session);
	if (res == SLATEERROR_OK) {
		res = slatew32_corewindow_fini(&local_history_window->core);
		DestroyWindow(local_history_window->core.hWnd);
		free(local_history_window);
		local_history_window = NULL;
	}

	return res;
}
