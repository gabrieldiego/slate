/*
 * Copyright 2010 John Mark Bell <jmb@netsurf-browser.org>
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
 * Implementation of GTK bookmark (hotlist) manager.
 */

#include <stdint.h>
#include <stdlib.h>
#include <gtk/gtk.h>

#include "utils/log.h"
#include "utils/slateoption.h"
#include "slate/keypress.h"
#include "slate/plotters.h"
#include "desktop/hotlist.h"

#include "gtk/compat.h"
#include "gtk/plotters.h"
#include "gtk/resources.h"
#include "gtk/corewindow.h"
#include "gtk/hotlist.h"

/**
 * hotlist window container for gtk.
 */
struct slategtk_hotlist_window {
	struct slategtk_corewindow core;
	GtkBuilder *builder;
	GtkWindow *wnd;
};

static struct slategtk_hotlist_window *hotlist_window = NULL;

#define MENUPROTO(x) static gboolean slategtk_on_##x##_activate( \
		GtkMenuItem *widget, gpointer g)
#define MENUEVENT(x) { #x, G_CALLBACK(slategtk_on_##x##_activate) }
#define MENUHANDLER(x) gboolean slategtk_on_##x##_activate(GtkMenuItem *widget, \
		gpointer g)

struct menu_events {
	const char *widget;
	GCallback handler;
};


/* file menu*/
MENUPROTO(export);
MENUPROTO(new_folder);
MENUPROTO(new_entry);

/* edit menu */
MENUPROTO(edit_selected);
MENUPROTO(delete_selected);
MENUPROTO(select_all);
MENUPROTO(clear_selection);

/* view menu*/
MENUPROTO(expand_all);
MENUPROTO(expand_directories);
MENUPROTO(expand_addresses);
MENUPROTO(collapse_all);
MENUPROTO(collapse_directories);
MENUPROTO(collapse_addresses);

MENUPROTO(launch);

static struct menu_events menu_events[] = {

	/* file menu*/
	MENUEVENT(export),
	MENUEVENT(new_folder),
	MENUEVENT(new_entry),

	/* edit menu */
	MENUEVENT(edit_selected),
	MENUEVENT(delete_selected),
	MENUEVENT(select_all),
	MENUEVENT(clear_selection),

	/* view menu*/
	MENUEVENT(expand_all),
	MENUEVENT(expand_directories),
	MENUEVENT(expand_addresses),
	MENUEVENT(collapse_all),
	MENUEVENT(collapse_directories),
	MENUEVENT(collapse_addresses),

	MENUEVENT(launch),

	{NULL, NULL}
};


/* file menu*/
MENUHANDLER(export)
{
	struct slategtk_hotlist_window *hlwin;
	GtkWidget *save_dialog;

	hlwin = (struct slategtk_hotlist_window *)g;

	save_dialog = gtk_file_chooser_dialog_new("Save File",
			hlwin->wnd,
			GTK_FILE_CHOOSER_ACTION_SAVE,
			SLATEGTK_STOCK_CANCEL, GTK_RESPONSE_CANCEL,
			SLATEGTK_STOCK_SAVE, GTK_RESPONSE_ACCEPT,
			NULL);

	gtk_file_chooser_set_current_folder(GTK_FILE_CHOOSER(save_dialog),
			getenv("HOME") ? getenv("HOME") : "/");

	gtk_file_chooser_set_current_name(GTK_FILE_CHOOSER(save_dialog),
			"hotlist.html");

	if (gtk_dialog_run(GTK_DIALOG(save_dialog)) == GTK_RESPONSE_ACCEPT) {
		gchar *filename = gtk_file_chooser_get_filename(
				GTK_FILE_CHOOSER(save_dialog));

		hotlist_export(filename, NULL);
		g_free(filename);
	}

	gtk_widget_destroy(save_dialog);

	return TRUE;
}

MENUHANDLER(new_folder)
{
	hotlist_add_folder(NULL, false, 0);
	return TRUE;
}

MENUHANDLER(new_entry)
{
	hotlist_add_entry(NULL, NULL, false, 0);
	return TRUE;
}

/* edit menu */
MENUHANDLER(edit_selected)
{
	hotlist_edit_selection();
	return TRUE;
}

MENUHANDLER(delete_selected)
{
	hotlist_keypress(NS_KEY_DELETE_LEFT);
	return TRUE;
}

MENUHANDLER(select_all)
{
	hotlist_keypress(NS_KEY_ESCAPE);
	hotlist_keypress(NS_KEY_ESCAPE);
	hotlist_keypress(NS_KEY_SELECT_ALL);
	return TRUE;
}

MENUHANDLER(clear_selection)
{
	hotlist_keypress(NS_KEY_CLEAR_SELECTION);
	return TRUE;
}

/* view menu*/
MENUHANDLER(expand_all)
{
	hotlist_expand(false);
	return TRUE;
}

MENUHANDLER(expand_directories)
{
	hotlist_expand(true);
	return TRUE;
}

MENUHANDLER(expand_addresses)
{
	hotlist_expand(false);
	return TRUE;
}

MENUHANDLER(collapse_all)
{
	hotlist_contract(true);
	return TRUE;
}

MENUHANDLER(collapse_directories)
{
	hotlist_contract(true);
	return TRUE;
}

MENUHANDLER(collapse_addresses)
{
	hotlist_contract(false);
	return TRUE;
}

MENUHANDLER(launch)
{
	hotlist_keypress(NS_KEY_CR);
	return TRUE;
}


/**
 * Connects menu events in the hotlist window.
 */
static void slategtk_hotlist_init_menu(struct slategtk_hotlist_window *hlwin)
{
	struct menu_events *event = menu_events;
	GtkWidget *w;

	while (event->widget != NULL) {
		w = GTK_WIDGET(gtk_builder_get_object(hlwin->builder,
						      event->widget));
		if (w == NULL) {
			NSLOG(netsurf, INFO,
			      "Unable to connect menu widget ""%s""",
			      event->widget);
		} else {
			g_signal_connect(G_OBJECT(w),
					 "activate",
					 event->handler,
					 hlwin);
		}
		event++;
	}
}


/**
 * callback for mouse action on hotlist window
 *
 * \param slategtk_cw The slategtk core window structure.
 * \param mouse_state netsurf mouse state on event
 * \param x location of event
 * \param y location of event
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slategtk_hotlist_mouse(struct slategtk_corewindow *slategtk_cw,
		    browser_mouse_state mouse_state,
		    int x, int y)
{
	hotlist_mouse_action(mouse_state, x, y);

	return SLATEERROR_OK;
}

/**
 * callback for keypress on hotlist window
 *
 * \param slategtk_cw The slategtk core window structure.
 * \param nskey The netsurf key code
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slategtk_hotlist_key(struct slategtk_corewindow *slategtk_cw, uint32_t nskey)
{
	if (hotlist_keypress(nskey)) {
		return SLATEERROR_OK;
	}
	return SLATEERROR_NOT_IMPLEMENTED;
}

/**
 * callback on draw event for hotlist window
 *
 * \param slategtk_cw The slategtk core window structure.
 * \param r The rectangle of the window that needs updating.
 * \return SLATEERROR_OK on success otherwise apropriate error code
 */
static slateerror
slategtk_hotlist_draw(struct slategtk_corewindow *slategtk_cw, struct rect *r)
{
	struct redraw_context ctx = {
		.interactive = true,
		.background_images = true,
		.plot = &slategtk_plotters
	};

	hotlist_redraw(0, 0, r, &ctx);

	return SLATEERROR_OK;
}

/**
 * Creates the window for the hotlist tree.
 *
 * \return SLATEERROR_OK on success else appropriate error code on faliure.
 */
static slateerror slategtk_hotlist_init(void)
{
	struct slategtk_hotlist_window *ncwin;
	slateerror res;

	if (hotlist_window != NULL) {
		return SLATEERROR_OK;
	}

	ncwin = calloc(1, sizeof(*ncwin));
	if (ncwin == NULL) {
		return SLATEERROR_NOMEM;
	}

	res = slategtk_builder_new_from_resname("hotlist", &ncwin->builder);
	if (res != SLATEERROR_OK) {
		NSLOG(netsurf, INFO, "Hotlist UI builder init failed");
		free(ncwin);
		return res;
	}

	gtk_builder_connect_signals(ncwin->builder, NULL);

	ncwin->wnd = GTK_WINDOW(gtk_builder_get_object(ncwin->builder,
						       "wndHotlist"));

	ncwin->core.scrolled = GTK_SCROLLED_WINDOW(
		gtk_builder_get_object(ncwin->builder, "hotlistScrolled"));

	ncwin->core.drawing_area = GTK_DRAWING_AREA(
		gtk_builder_get_object(ncwin->builder, "hotlistDrawingArea"));

	/* make the delete event hide the window */
	g_signal_connect(G_OBJECT(ncwin->wnd),
			 "delete_event",
			 G_CALLBACK(gtk_widget_hide_on_delete),
			 NULL);

	slategtk_hotlist_init_menu(ncwin);

	ncwin->core.draw = slategtk_hotlist_draw;
	ncwin->core.key = slategtk_hotlist_key;
	ncwin->core.mouse = slategtk_hotlist_mouse;

	res = slategtk_corewindow_init(&ncwin->core);
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


/* exported function documented gtk/hotlist.h */
slateerror slategtk_hotlist_present(void)
{
	slateerror res;

	res = slategtk_hotlist_init();
	if (res == SLATEERROR_OK) {
		gtk_window_present(hotlist_window->wnd);
	}
	return res;
}


/* exported function documented gtk/hotlist.h */
slateerror slategtk_hotlist_destroy(void)
{
	slateerror res;

	if (hotlist_window == NULL) {
		return SLATEERROR_OK;
	}

	res = hotlist_manager_fini();
	if (res == SLATEERROR_OK) {
		res = slategtk_corewindow_fini(&hotlist_window->core);
		gtk_widget_destroy(GTK_WIDGET(hotlist_window->wnd));
		g_object_unref(G_OBJECT(hotlist_window->builder));
		free(hotlist_window);
		hotlist_window = NULL;
	}

	return res;
}
