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

#ifndef SLATE_WINDOWS_COREWINDOW_H
#define SLATE_WINDOWS_COREWINDOW_H

#include "slate/core_window.h"

extern struct core_window_table *win32_core_window_table;

/**
 * slatew32 core window state
 */
struct slatew32_corewindow {
	/** window handle */
	HWND hWnd;

	/** content width */
	int content_width;

	/** content height */
	int content_height;

	/** window title */
	const char *title;
	
        /** drag status set by core */
        core_window_drag_status drag_status;
	
        /**
         * callback to draw on drawable area of slatew32 core window
         *
         * \param slatew32_cw The slatew32 core window structure.
         * \param r The rectangle of the window that needs updating.
         * \return SLATEERROR_OK on success otherwise apropriate error code
         */
        slateerror (*draw)(struct slatew32_corewindow *slatew32_cw, int scrollx, int scrolly, struct rect *r);

        /**
         * callback for keypress on slatew32 core window
         *
         * \param slatew32_cw The slatew32 core window structure.
         * \param nskey The netsurf key code.
         * \return SLATEERROR_OK if key processed,
         *         SLATEERROR_NOT_IMPLEMENTED if key not processed
         *         otherwise apropriate error code
         */
        slateerror (*key)(struct slatew32_corewindow *slatew32_cw, uint32_t nskey);

        /**
         * callback for mouse event on slatew32 core window
         *
         * \param slatew32_cw The slatew32 core window structure.
         * \param mouse_state mouse state
         * \param x location of event
         * \param y location of event
         * \return SLATEERROR_OK on sucess otherwise apropriate error code.
         */
        slateerror (*mouse)(struct slatew32_corewindow *slatew32_cw, browser_mouse_state mouse_state, int x, int y);

	/**
	 * callback for window close event
	 *
         * \param slatew32_cw The slatew32 core window structure.
         * \return SLATEERROR_OK on sucess otherwise apropriate error code.
	 */
	slateerror (*close)(struct slatew32_corewindow *slatew32_cw);
};

/**
 * initialise elements of slatew32 core window.
 *
 * As a pre-requisite the draw, key and mouse callbacks must be defined
 *
 * \param hInstance The instance to create the core window in
 * \param hWndParent parent window handle may be NULL for top level window.
 * \param slatew32_cw A slatew32 core window structure to initialise
 * \return SLATEERROR_OK on successful initialisation otherwise error code.
 */
slateerror slatew32_corewindow_init(HINSTANCE hInstance,
			      HWND hWndParent,
			      struct slatew32_corewindow *slatew32_cw);

/**
 * finalise elements of slatew32 core window.
 *
 * \param slatew32_cw A slatew32 core window structure to initialise
 * \return SLATEERROR_OK on successful finalisation otherwise error code.
 */
slateerror slatew32_corewindow_fini(struct slatew32_corewindow *slatew32_cw);

slateerror slatew32_create_corewindow_class(HINSTANCE hInstance);

#endif
