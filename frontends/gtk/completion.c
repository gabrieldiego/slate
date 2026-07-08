/*
 * Copyright 2006 Rob Kendrick <rjek@rjek.com>
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
 * Implementation of url entry completion.
 */

#include <stdlib.h>

#include "utils/log.h"
#include "utils/messages.h"
#include "utils/slateoption.h"
#include "utils/slateurl.h"
#include "slate/url_db.h"
#include "slate/browser_window.h"
#include "desktop/searchweb.h"

#include "gtk/compat.h"
#include "gtk/warn.h"
#include "gtk/scaffolding.h"
#include "gtk/toolbar_items.h"
#include "gtk/window.h"
#include "gtk/completion.h"

GtkListStore *slategtk_completion_list;

struct slategtk_completion_ctx {
	/**
	 * callback to obtain a browser window for navigation
	 */
	struct browser_window *(*get_bw)(void *ctx);

	/**
	 * context passed to get_bw function
	 */
	void *get_bw_ctx;
};

/**
 * completion row matcher
 */
static gboolean slategtk_completion_match(GtkEntryCompletion *completion,
				const gchar *key,
				GtkTreeIter *iter,
				gpointer user_data)
{
	/* the completion list is modified to only contain valid
	 * entries so this simply returns TRUE to indicate all rows
	 * are in the list should be shown.
	 */
	return TRUE;
}


/**
 * callback for each entry to add to completion list
 */
static bool
slategtk_completion_udb_callback(slateurl *url, const struct url_data *data)
{
	GtkTreeIter iter;

	if (data->visits != 0) {
		gtk_list_store_append(slategtk_completion_list, &iter);
		gtk_list_store_set(slategtk_completion_list, &iter, 0,
				slateurl_access(url), -1);
	}
	return true;
}

/**
 * event handler for when a completion suggestion is selected.
 */
static gboolean
slategtk_completion_match_select(GtkEntryCompletion *widget,
			      GtkTreeModel *model,
			      GtkTreeIter *iter,
			      gpointer data)
{
	struct slategtk_completion_ctx *cb_ctx;
	GValue value = G_VALUE_INIT;
	struct browser_window *bw;
	slateerror ret;
	slateurl *url;

	cb_ctx = data;
	bw = cb_ctx->get_bw(cb_ctx->get_bw_ctx);

	gtk_tree_model_get_value(model, iter, 0, &value);

	ret = search_web_omni(g_value_get_string(&value),
			      SEARCH_WEB_OMNI_NONE,
			      &url);

	g_value_unset(&value);

	if (ret == SLATEERROR_OK) {
		ret = browser_window_navigate(bw,
					      url, NULL, BW_NAVIGATE_HISTORY,
					      NULL, NULL, NULL);
		slateurl_unref(url);
	}
	if (ret != SLATEERROR_OK) {
		slategtk_warning(messages_get_errorcode(ret), 0);
	}

	return TRUE;
}

/* exported interface documented in completion.h */
void slategtk_completion_init(void)
{
	slategtk_completion_list = gtk_list_store_new(1, G_TYPE_STRING);

}

/* exported interface documented in completion.h */
gboolean slategtk_completion_update(GtkEntry *entry)
{
	gtk_list_store_clear(slategtk_completion_list);

	if (slateoption_bool(url_suggestion) == true) {
		urldb_iterate_partial(gtk_entry_get_text(entry),
				      slategtk_completion_udb_callback);
	}

	return TRUE;
}

/* exported interface documented in completion.h */
slateerror
slategtk_completion_connect_signals(GtkEntry *entry,
				 struct browser_window *(*get_bw)(void *ctx),
				 void *get_bw_ctx)
{
	GtkEntryCompletion *completion;
	struct slategtk_completion_ctx *cb_ctx;

	cb_ctx = calloc(1, sizeof(struct slategtk_completion_ctx));
	if (cb_ctx == NULL) {
		return SLATEERROR_NOMEM;
	}

	cb_ctx->get_bw = get_bw;
	cb_ctx->get_bw_ctx = get_bw_ctx;

	completion = gtk_entry_get_completion(entry);

	gtk_entry_completion_set_match_func(completion,
			slategtk_completion_match, NULL, NULL);

	gtk_entry_completion_set_model(completion,
			GTK_TREE_MODEL(slategtk_completion_list));

	gtk_entry_completion_set_text_column(completion, 0);

	gtk_entry_completion_set_minimum_key_length(completion, 1);

	/* enable popup for completion */
	gtk_entry_completion_set_popup_completion(completion, TRUE);

	/* when selected callback */
	g_signal_connect(G_OBJECT(completion),
			 "match-selected",
			 G_CALLBACK(slategtk_completion_match_select),
			 cb_ctx);

	g_object_set(G_OBJECT(completion),
		     "popup-set-width", TRUE,
		     "popup-single-match", TRUE,
		     NULL);

	return SLATEERROR_OK;
}
