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

#ifndef GTK_COREWINDOW_H
#define GTK_COREWINDOW_H

#include "slate/core_window.h"

extern struct core_window_table *slategtk_core_window_table;

/**
 * slategtk core window mouse state
 */
struct slategtk_corewindow_mouse {
	browser_mouse_state state; /**< last event status */
	bool pressed;
	int pressed_x;
	int pressed_y;
	int last_x;
	int last_y;
};

/**
 * slategtk core window state
 */
struct slategtk_corewindow {
	/* public variables */
	/** GTK drawable widget */
	GtkDrawingArea *drawing_area;
	/** scrollable area drawing area is within */
	GtkScrolledWindow *scrolled;

	/* private variables */
	/** Input method */
	GtkIMContext *input_method;

	/** mouse state */
	struct slategtk_corewindow_mouse mouse_state;

	/** drag status set by core */
	core_window_drag_status drag_status;

	/**
	 * callback to draw on drawable area of slategtk core window
	 *
	 * \param slategtk_cw The slategtk core window structure.
	 * \param r The rectangle of the window that needs updating.
	 * \return SLATEERROR_OK on success otherwise appropriate error code
	 */
	slateerror (*draw)(struct slategtk_corewindow *slategtk_cw, struct rect *r);

	/**
	 * callback for keypress on slategtk core window
	 *
	 * \param slategtk_cw The slategtk core window structure.
	 * \param nskey The netsurf key code.
	 * \return SLATEERROR_OK if key processed,
	 *         SLATEERROR_NOT_IMPLEMENTED if key not processed
	 *         otherwise appropriate error code
	 */
	slateerror (*key)(struct slategtk_corewindow *slategtk_cw, uint32_t nskey);

	/**
	 * callback for mouse event on slategtk core window
	 *
	 * \param slategtk_cw The slategtk core window structure.
	 * \param mouse_state mouse state
	 * \param x location of event
	 * \param y location of event
	 * \return SLATEERROR_OK on success otherwise appropriate error code.
	 */
	slateerror (*mouse)(struct slategtk_corewindow *slategtk_cw, browser_mouse_state mouse_state, int x, int y);
};

/**
 * initialise elements of gtk core window.
 *
 * \param slategtk_cw A gtk core window structure to initialise
 * \return SLATEERROR_OK on successful initialisation otherwise error code.
 */
slateerror slategtk_corewindow_init(struct slategtk_corewindow *slategtk_cw);

/**
 * finalise elements of gtk core window.
 *
 * \param slategtk_cw A gtk core window structure to initialise
 * \return SLATEERROR_OK on successful finalisation otherwise error code.
 */
slateerror slategtk_corewindow_fini(struct slategtk_corewindow *slategtk_cw);

#endif
