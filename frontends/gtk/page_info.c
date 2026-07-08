/*
 * Copyright 2015 Vincent Sanders <vince@netsurf-browser.org>
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
 * Implementation of gtk certificate viewing using gtk core windows.
 */

#include <stdint.h>
#include <stdlib.h>
#include <gtk/gtk.h>

#include "utils/log.h"
#include "utils/messages.h"
#include "slate/keypress.h"
#include "slate/plotters.h"
#include "slate/misc.h"
#include "slate/browser_window.h"
#include "desktop/page-info.h"
#include "desktop/gui_internal.h"

#include "gtk/plotters.h"
#include "gtk/scaffolding.h"
#include "gtk/resources.h"
#include "gtk/page_info.h"
#include "gtk/corewindow.h"


/**
 * GTK certificate viewing window context
 */
struct slategtk_pi_window {
	/** GTK core window context */
	struct slategtk_corewindow core;
	/** GTK builder for window */
	GtkBuilder *builder;
	/** GTK window being shown */
	GtkWindow *dlg;
	/** Core page-info window */
	struct page_info *pi;
};


/**
 * destroy a previously created page information window
 */
static gboolean
slategtk_pi_delete_event(GtkWidget *w, GdkEvent  *event, gpointer data)
{
	struct slategtk_pi_window *pi_win;
	pi_win = (struct slategtk_pi_window *)data;

	page_info_destroy(pi_win->pi);

	slategtk_corewindow_fini(&pi_win->core);
	gtk_widget_destroy(GTK_WIDGET(pi_win->dlg));
	g_object_unref(G_OBJECT(pi_win->builder));
	free(pi_win);

	return FALSE;
}

/**
 * Called to cause the page-info window to close cleanly
 */
static void
slategtk_pi_close_callback(void *pw)
{
	slategtk_pi_delete_event(NULL, NULL, pw);
}

/**
 * callback for mouse action for certificate verify on core window
 *
 * \param slategtk_cw The slategtk core window structure.
 * \param mouse_state netsurf mouse state on event
 * \param x location of event
 * \param y location of event
 * \return SLATEERROR_OK on success otherwise appropriate error code
 */
static slateerror
slategtk_pi_mouse(struct slategtk_corewindow *slategtk_cw,
		    browser_mouse_state mouse_state,
		    int x, int y)
{
	struct slategtk_pi_window *pi_win;
	bool did_something = false;
	/* technically degenerate container of */
	pi_win = (struct slategtk_pi_window *)slategtk_cw;

	if (page_info_mouse_action(pi_win->pi, mouse_state, x, y, &did_something) == SLATEERROR_OK) {
		if (did_something == true) {
			/* Something happened so we need to close ourselves */
			guit->misc->schedule(0, slategtk_pi_close_callback, pi_win);
		}
	}

	return SLATEERROR_OK;
}

/**
 * callback for keypress for certificate verify on core window
 *
 * \param slategtk_cw The slategtk core window structure.
 * \param nskey The netsurf key code
 * \return SLATEERROR_OK on success otherwise appropriate error code
 */
static slateerror
slategtk_pi_key(struct slategtk_corewindow *slategtk_cw, uint32_t nskey)
{
	struct slategtk_pi_window *pi_win;

	/* technically degenerate container of */
	pi_win = (struct slategtk_pi_window *)slategtk_cw;

	if (page_info_keypress(pi_win->pi, nskey)) {
		return SLATEERROR_OK;
	}
	return SLATEERROR_NOT_IMPLEMENTED;
}

/**
 * callback on draw event for certificate verify on core window
 *
 * \param slategtk_cw The slategtk core window structure.
 * \param r The rectangle of the window that needs updating.
 * \return SLATEERROR_OK on success otherwise appropriate error code
 */
static slateerror
slategtk_pi_draw(struct slategtk_corewindow *slategtk_cw, struct rect *r)
{
	struct redraw_context ctx = {
		.interactive = true,
		.background_images = true,
		.plot = &slategtk_plotters
	};
	struct slategtk_pi_window *pi_win;

	/* technically degenerate container of */
	pi_win = (struct slategtk_pi_window *)slategtk_cw;

	page_info_redraw(pi_win->pi, 0, 0, r, &ctx);

	return SLATEERROR_OK;
}

/* exported interface documented in gtk/page_info.h */
slateerror slategtk_page_info(struct browser_window *bw)
{
	struct slategtk_pi_window *ncwin;
	slateerror res;
	GtkWindow *scaffwin = slategtk_scaffolding_window(slategtk_current_scaffolding());

	ncwin = calloc(1, sizeof(struct slategtk_pi_window));
	if (ncwin == NULL) {
		return SLATEERROR_NOMEM;
	}

	res = slategtk_builder_new_from_resname("pageinfo", &ncwin->builder);
	if (res != SLATEERROR_OK) {
		NSLOG(netsurf, CRITICAL, "Page Info UI builder init failed %s", messages_get_errorcode(res));
		free(ncwin);
		return res;
	}

	gtk_builder_connect_signals(ncwin->builder, NULL);

	ncwin->dlg = GTK_WINDOW(gtk_builder_get_object(ncwin->builder,
						       "PGIWindow"));

	/* Configure for transient behaviour */
	gtk_window_set_type_hint(GTK_WINDOW(ncwin->dlg),
				 GDK_WINDOW_TYPE_HINT_DROPDOWN_MENU);

	gtk_window_set_modal(GTK_WINDOW(ncwin->dlg), TRUE);

	gtk_window_group_add_window(gtk_window_get_group(scaffwin),
				    GTK_WINDOW(ncwin->dlg));

	gtk_window_set_transient_for(GTK_WINDOW(ncwin->dlg), scaffwin);

	gtk_window_set_screen(GTK_WINDOW(ncwin->dlg),
			      gtk_widget_get_screen(GTK_WIDGET(scaffwin)));

	/* Attempt to place the window in the right place */
	slategtk_scaffolding_position_page_info(slategtk_current_scaffolding(),
					     ncwin);

	ncwin->core.drawing_area = GTK_DRAWING_AREA(
		gtk_builder_get_object(ncwin->builder, "PGIDrawingArea"));

	/* make the delete event call our destructor */
	g_signal_connect(G_OBJECT(ncwin->dlg),
			 "delete_event",
			 G_CALLBACK(slategtk_pi_delete_event),
			 ncwin);
	/* Ditto if we lose the grab */
	g_signal_connect(G_OBJECT(ncwin->dlg),
			 "grab-broken-event",
			 G_CALLBACK(slategtk_pi_delete_event),
			 ncwin);
	/* Handle button press events */
	g_signal_connect(G_OBJECT(ncwin->dlg),
			 "button-press-event",
			 G_CALLBACK(slategtk_pi_delete_event),
			 ncwin);

	/* initialise GTK core window */
	ncwin->core.draw = slategtk_pi_draw;
	ncwin->core.key = slategtk_pi_key;
	ncwin->core.mouse = slategtk_pi_mouse;

	res = slategtk_corewindow_init(&ncwin->core);
	if (res != SLATEERROR_OK) {
		g_object_unref(G_OBJECT(ncwin->dlg));
		free(ncwin);
		return res;
	}

	res = page_info_create((struct core_window *)ncwin, bw, &ncwin->pi);
	if (res != SLATEERROR_OK) {
		g_object_unref(G_OBJECT(ncwin->dlg));
		free(ncwin);
		return res;
	}

	gtk_widget_show(GTK_WIDGET(ncwin->dlg));

	gtk_widget_grab_focus(GTK_WIDGET(ncwin->dlg));

	return SLATEERROR_OK;
}

/* exported interface documented in gtk/page_info.h */
void
slategtk_page_info_set_position(struct slategtk_pi_window *win, int x, int y)
{
	NSLOG(netsurf, INFO, "win=%p x=%d y=%d", win, x, y);

	gtk_window_move(GTK_WINDOW(win->dlg), x, y);
}
