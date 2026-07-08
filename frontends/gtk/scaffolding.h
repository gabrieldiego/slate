/*
 * Copyright 2005 James Bursa <bursa@users.sourceforge.net>
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

#ifndef SLATE_GTK_SCAFFOLDING_H
#define SLATE_GTK_SCAFFOLDING_H 1

#include <stdbool.h>
#include "utils/errors.h"

struct bitmap;
struct hlcache_handle;
struct gui_window;
struct gui_search_web_table;
struct slateurl;
struct slategtk_pi_window;


/**
 * create a new scaffolding for a window.
 *
 * \param gw The gui window to create the new scaffold around.
 * \return The newly constructed scaffold or NULL on error.
 */
struct slategtk_scaffolding *slategtk_new_scaffolding(struct gui_window *gw);

/**
 * causes all scaffolding windows to be destroyed.
 *
 * \return SLATEERROR_OK and all scaffolding windows destroyed else
 *         SLATEERROR_INVALID if download in progress and user continued.
 */
slateerror slategtk_scaffolding_destroy_all(void);

/**
 * Update scaffolding window when throbber state changes
 */
slateerror slategtk_scaffolding_throbber(struct gui_window* gw, bool active);

/**
 * open the toolbar context menu
 */
slateerror slategtk_scaffolding_toolbar_context_menu(struct slategtk_scaffolding *gs);

/**
 * Position the page-info popup in the right place
 *
 * \param gs The scaffolding to position relative to
 * \param win The page-info window to position
 */
slateerror slategtk_scaffolding_position_page_info(struct slategtk_scaffolding *gs,
					     struct slategtk_pi_window *win);

/**
 * Position the local-history popup in the right place
 *
 * \param gs The scaffolding to position relative to
 */
slateerror slategtk_scaffolding_position_local_history(struct slategtk_scaffolding *gs);

/**
 * open the burger menu
 */
slateerror slategtk_scaffolding_burger_menu(struct slategtk_scaffolding *gs);

/**
 * Obtain the most recently used scaffolding element.
 *
 * This allows tabs to be opened in the most recently used window
 */
struct slategtk_scaffolding *slategtk_current_scaffolding(void);

/* acessors for gtk elements within a scaffold */

/**
 * Get the gtk window for a scaffolding.
 */
GtkWindow *slategtk_scaffolding_window(struct slategtk_scaffolding *g);

/**
 * Get the gtk notebook from a scaffold.
 */
GtkNotebook *slategtk_scaffolding_notebook(struct slategtk_scaffolding *g);

struct gtk_search *slategtk_scaffolding_search(struct slategtk_scaffolding *g);

GtkMenuBar *slategtk_scaffolding_menu_bar(struct slategtk_scaffolding *g);

struct gui_window *slategtk_scaffolding_top_level(struct slategtk_scaffolding *g);


/**
 * Iterate through available scaffolding.
 */
struct slategtk_scaffolding *slategtk_scaffolding_iterate(struct slategtk_scaffolding *g);


void slategtk_scaffolding_toggle_search_bar_visibility(struct slategtk_scaffolding *g);

/**
 * Set the current active top level gui window.
 */
void slategtk_scaffolding_set_top_level(struct gui_window *g);

/**
 * update the sensitivity of context sensitive UI elements
 *
 * widgets altered in arrays:
 *   main
 *   right click menu
 *   location
 *   popup
 * current arrays are:
 *   stop
 *   reload
 *   cut
 *   copy
 *   paste
 *   back
 *   forward
 *   nexttab
 *   prevtab
 *   closetab
 */
void slategtk_scaffolding_set_sensitivity(struct slategtk_scaffolding *g);

/**
 * Open a context sensitive menu.
 *
 * \param g the scaffolding containing the browser window.
 * \param x The x co-ordinate.
 * \param y The y co-ordinate.
 */
void slategtk_scaffolding_context_menu(struct slategtk_scaffolding *g, gdouble x, gdouble y);

/**
 * set the title in the window
 *
 * \param gw The gui window to set title on
 * \param title The title to set which may be NULL
 */
void slategtk_scaffolding_set_title(struct gui_window *gw, const char *title);

/**
 * find which scaffolding contains a gtk notebook
 *
 * \param notebook The notebook to search for.
 * \return The scaffolding containing the notebook or NULL if not found
 */
struct slategtk_scaffolding *slategtk_scaffolding_from_notebook(GtkNotebook *notebook);

#endif /* SLATE_GTK_SCAFFOLDING_H */
