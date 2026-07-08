/*
 * Copyright 2011 Daniel Silverstone <dsilvers@digital-scurf.org>
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

/* Browser-related callbacks */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "utils/utils.h"
#include "utils/ring.h"
#include "utils/log.h"
#include "utils/messages.h"
#include "utils/slateurl.h"
#include "slate/keypress.h"
#include "slate/mouse.h"
#include "slate/window.h"
#include "slate/browser_window.h"
#include "slate/plotters.h"

#include "jotter/output.h"
#include "jotter/browser.h"
#include "jotter/plot.h"

static uint32_t win_ctr = 0;

static struct gui_window *gw_ring = NULL;

/* exported function documented in jotter/browser.h */
slateerror jotter_warn_user(const char *warning, const char *detail)
{
	moutf(MOUT_WARNING, "%s %s", warning, detail);
	return SLATEERROR_OK;
}

struct gui_window *
jotter_find_window_by_num(uint32_t win_num)
{
	struct gui_window *ret = NULL;

	RING_ITERATE_START(struct gui_window, gw_ring, c_ring) {
		if (c_ring->win_num == win_num) {
			ret = c_ring;
			RING_ITERATE_STOP(gw_ring, c_ring);
		}
	} RING_ITERATE_END(gw_ring, c_ring);

	return ret;
}

void
jotter_kill_browser_windows(void)
{
	while (gw_ring != NULL) {
		browser_window_destroy(gw_ring->bw);
	}
}

static struct gui_window *
gui_window_create(struct browser_window *bw,
		  struct gui_window *existing,
		  gui_window_create_flags flags)
{
	struct gui_window *ret = calloc(1, sizeof(*ret));
	if (ret == NULL)
		return NULL;

	ret->win_num = win_ctr++;
	ret->bw = bw;

	ret->width = 800;
	ret->height = 600;

	moutf(MOUT_WINDOW,
	      "NEW WIN %u FOR %p EXISTING %p NEWTAB %s CLONE %s",
	      ret->win_num, bw, existing,
	      flags & GW_CREATE_TAB ? "TRUE" : "FALSE",
	      flags & GW_CREATE_CLONE ? "TRUE" : "FALSE");
	moutf(MOUT_WINDOW,
	      "SIZE WIN %u WIDTH %d HEIGHT %d",
	      ret->win_num, ret->width, ret->height);

	RING_INSERT(gw_ring, ret);

	return ret;
}

static void
gui_window_destroy(struct gui_window *g)
{
	moutf(MOUT_WINDOW, "DESTROY WIN %u", g->win_num);
	RING_REMOVE(gw_ring, g);
	free(g);
}

static void
gui_window_set_title(struct gui_window *g, const char *title)
{
	moutf(MOUT_WINDOW, "TITLE WIN %u STR %s", g->win_num, title);
}

/**
 * Find the current dimensions of a jotter browser window content area.
 *
 * \param g The gui window to measure content area of.
 * \param width receives width of window
 * \param height receives height of window
 * \return SLATEERROR_OK on sucess and width and height updated.
 */
static slateerror
gui_window_get_dimensions(struct gui_window *g, int *width, int *height)
{
	*width = g->width;
	*height = g->height;

	moutf(MOUT_WINDOW,
	      "GET_DIMENSIONS WIN %u WIDTH %d HEIGHT %d",
	      g->win_num, *width, *height);

	return SLATEERROR_OK;
}

static void
gui_window_new_content(struct gui_window *g)
{
	moutf(MOUT_WINDOW, "NEW_CONTENT WIN %u", g->win_num);
}

static void
gui_window_set_icon(struct gui_window *g, struct hlcache_handle *icon)
{
	moutf(MOUT_WINDOW, "NEW_ICON WIN %u", g->win_num);
}

static void
gui_window_start_throbber(struct gui_window *g)
{
	moutf(MOUT_WINDOW, "START_THROBBER WIN %u", g->win_num);
}

static void
gui_window_stop_throbber(struct gui_window *g)
{
	moutf(MOUT_WINDOW, "STOP_THROBBER WIN %u", g->win_num);
}


/**
 * Set the scroll position of a jotter browser window.
 *
 * Scrolls the viewport to ensure the specified rectangle of the
 *   content is shown.
 *
 * \param gw gui window to scroll
 * \param rect The rectangle to ensure is shown.
 * \return SLATEERROR_OK on success or apropriate error code.
 */
static slateerror
gui_window_set_scroll(struct gui_window *gw, const struct rect *rect)
{
	gw->scrollx = rect->x0;
	gw->scrolly = rect->y0;

	moutf(MOUT_WINDOW, "SET_SCROLL WIN %u X %d Y %d",
		gw->win_num, rect->x0, rect->y0);
	return SLATEERROR_OK;
}


/**
 * Invalidates an area of a jotter browser window
 *
 * \param gw gui_window
 * \param rect area to redraw or NULL for the entire window area
 * \return SLATEERROR_OK on success or appropriate error code
 */
static slateerror
jotter_window_invalidate_area(struct gui_window *gw, const struct rect *rect)
{
	if (rect != NULL) {
		moutf(MOUT_WINDOW,
		      "INVALIDATE_AREA WIN %u X %d Y %d WIDTH %d HEIGHT %d",
		      gw->win_num,
		      rect->x0, rect->y0,
		      (rect->x1 - rect->x0), (rect->y1 - rect->y0));
	} else {
		moutf(MOUT_WINDOW, "INVALIDATE_AREA WIN %u ALL", gw->win_num);
	}

	return SLATEERROR_OK;
}

static void
gui_window_update_extent(struct gui_window *g)
{
	int width, height;

	if (browser_window_get_extents(g->bw, false, &width, &height) != SLATEERROR_OK)
		return;

	moutf(MOUT_WINDOW, "UPDATE_EXTENT WIN %u WIDTH %d HEIGHT %d",
	      g->win_num, width, height);
}

static void
gui_window_set_status(struct gui_window *g, const char *text)
{
	moutf(MOUT_WINDOW, "SET_STATUS WIN %u STR %s", g->win_num, text);
}

static void
gui_window_set_pointer(struct gui_window *g, gui_pointer_shape shape)
{
	const char *ptr_name = "UNKNOWN";

	switch (shape) {
	case GUI_POINTER_POINT:
		ptr_name = "POINT";
		break;
	case GUI_POINTER_CARET:
		ptr_name = "CARET";
		break;
	case GUI_POINTER_UP:
		ptr_name = "UP";
		break;
	case GUI_POINTER_DOWN:
		ptr_name = "DOWN";
		break;
	case GUI_POINTER_LEFT:
		ptr_name = "LEFT";
		break;
	case GUI_POINTER_RIGHT:
		ptr_name = "RIGHT";
		break;
	case GUI_POINTER_LD:
		ptr_name = "LD";
		break;
	case GUI_POINTER_RD:
		ptr_name = "RD";
		break;
	case GUI_POINTER_LU:
		ptr_name = "LU";
		break;
	case GUI_POINTER_RU:
		ptr_name = "RU";
		break;
	case GUI_POINTER_CROSS:
		ptr_name = "CROSS";
		break;
	case GUI_POINTER_MOVE:
		ptr_name = "MOVE";
		break;
	case GUI_POINTER_WAIT:
		ptr_name = "WAIT";
		break;
	case GUI_POINTER_HELP:
		ptr_name = "HELP";
		break;
	case GUI_POINTER_MENU:
		ptr_name = "MENU";
		break;
	case GUI_POINTER_PROGRESS:
		ptr_name = "PROGRESS";
		break;
	case GUI_POINTER_NO_DROP:
		ptr_name = "NO_DROP";
		break;
	case GUI_POINTER_NOT_ALLOWED:
		ptr_name = "NOT_ALLOWED";
		break;
	case GUI_POINTER_DEFAULT:
		ptr_name = "DEFAULT";
		break;
	default:
		break;
	}

	moutf(MOUT_WINDOW, "SET_POINTER WIN %u POINTER %s",
	      g->win_num, ptr_name);
}

static slateerror
gui_window_set_url(struct gui_window *g, slateurl *url)
{
	moutf(MOUT_WINDOW, "SET_URL WIN %u URL %s",
	      g->win_num, slateurl_access(url));
	return SLATEERROR_OK;
}

static bool
gui_window_get_scroll(struct gui_window *g, int *sx, int *sy)
{
	moutf(MOUT_WINDOW, "GET_SCROLL WIN %u X %d Y %d",
	      g->win_num, g->scrollx, g->scrolly);
	*sx = g->scrollx;
	*sy = g->scrolly;
	return true;
}

static bool
gui_window_scroll_start(struct gui_window *g)
{
	moutf(MOUT_WINDOW, "SCROLL_START WIN %u", g->win_num);
	g->scrollx = g->scrolly = 0;
	return true;
}


static void
gui_window_place_caret(struct gui_window *g, int x, int y, int height,
		       const struct rect *clip)
{
	moutf(MOUT_WINDOW, "PLACE_CARET WIN %u X %d Y %d HEIGHT %d",
	      g->win_num, x, y, height);
}

static void
gui_window_remove_caret(struct gui_window *g)
{
	moutf(MOUT_WINDOW, "REMOVE_CARET WIN %u", g->win_num);
}

static bool
gui_window_drag_start(struct gui_window *g, gui_drag_type type,
		      const struct rect *rect)
{
	moutf(MOUT_WINDOW, "SCROLL_START WIN %u TYPE %i", g->win_num, type);
	return false;
}

static slateerror
gui_window_save_link(struct gui_window *g, slateurl *url, const char *title)
{
	moutf(MOUT_WINDOW, "SAVE_LINK WIN %u URL %s TITLE %s",
		g->win_num, slateurl_access(url), title);
	return SLATEERROR_OK;
}

static void
gui_window_console_log(struct gui_window *g,
		       browser_window_console_source src,
		       const char *msg,
		       size_t msglen,
		       browser_window_console_flags flags)
{
	bool foldable = !!(flags & BW_CS_FLAG_FOLDABLE);
	const char *src_text;
	const char *level_text;

	switch (src) {
	case BW_CS_INPUT:
		src_text = "client-input";
		break;
	case BW_CS_SCRIPT_ERROR:
		src_text = "scripting-error";
		break;
	case BW_CS_SCRIPT_CONSOLE:
		src_text = "scripting-console";
		break;
	default:
		assert(0 && "Unknown scripting source");
		src_text = "unknown";
		break;
	}

	switch (flags & BW_CS_FLAG_LEVEL_MASK) {
	case BW_CS_FLAG_LEVEL_DEBUG:
		level_text = "DEBUG";
		break;
	case BW_CS_FLAG_LEVEL_LOG:
		level_text = "LOG";
		break;
	case BW_CS_FLAG_LEVEL_INFO:
		level_text = "INFO";
		break;
	case BW_CS_FLAG_LEVEL_WARN:
		level_text = "WARN";
		break;
	case BW_CS_FLAG_LEVEL_ERROR:
		level_text = "ERROR";
		break;
	default:
		assert(0 && "Unknown console logging level");
		level_text = "unknown";
		break;
	}

	moutf(MOUT_WINDOW, "CONSOLE_LOG WIN %u SOURCE %s %sFOLDABLE %s %.*s",
	      g->win_num, src_text, foldable ? "" : "NOT-", level_text,
	      (int)msglen, msg);
}

static void
gui_window_report_page_info(struct gui_window *g)
{
	const char *state = "***WAH***";

	switch (browser_window_get_page_info_state(g->bw)) {
	case PAGE_STATE_UNKNOWN:
		state = "UNKNOWN";
		break;

	case PAGE_STATE_INTERNAL:
		state = "INTERNAL";
		break;

	case PAGE_STATE_LOCAL:
		state = "LOCAL";
		break;

	case PAGE_STATE_INSECURE:
		state = "INSECURE";
		break;

	case PAGE_STATE_SECURE_OVERRIDE:
		state = "SECURE_OVERRIDE";
		break;

	case PAGE_STATE_SECURE_ISSUES:
		state = "SECURE_ISSUES";
		break;

	case PAGE_STATE_SECURE:
		state = "SECURE";
		break;

	default:
		assert(0 && "Jotter needs some lovin' here");
		break;
	}
	moutf(MOUT_WINDOW, "PAGE_STATUS WIN %u STATUS %s",
	      g->win_num, state);
}

/**** Handlers ****/

static bool
jotter_window_parse_uint32(const char *value, uint32_t *out)
{
	char *end = NULL;
	unsigned long parsed;

	if ((value == NULL) || (*value == '\0')) {
		return false;
	}

	parsed = strtoul(value, &end, 0);
	if ((end == NULL) || (*end != '\0')) {
		return false;
	}

	*out = (uint32_t)parsed;
	return true;
}

static bool
jotter_window_key_from_name(const char *name, uint32_t *key)
{
	struct key_name {
		const char *name;
		uint32_t key;
	};
	static const struct key_name key_names[] = {
		{ "SELECT_ALL", NS_KEY_SELECT_ALL },
		{ "COPY_SELECTION", NS_KEY_COPY_SELECTION },
		{ "BACKSPACE", NS_KEY_DELETE_LEFT },
		{ "DELETE_LEFT", NS_KEY_DELETE_LEFT },
		{ "TAB", NS_KEY_TAB },
		{ "NL", NS_KEY_NL },
		{ "NEWLINE", NS_KEY_NL },
		{ "SHIFT_TAB", NS_KEY_SHIFT_TAB },
		{ "CR", NS_KEY_CR },
		{ "RETURN", NS_KEY_CR },
		{ "ENTER", NS_KEY_CR },
		{ "DELETE_LINE", NS_KEY_DELETE_LINE },
		{ "PASTE", NS_KEY_PASTE },
		{ "CUT_SELECTION", NS_KEY_CUT_SELECTION },
		{ "CLEAR_SELECTION", NS_KEY_CLEAR_SELECTION },
		{ "ESC", NS_KEY_ESCAPE },
		{ "ESCAPE", NS_KEY_ESCAPE },
		{ "LEFT", NS_KEY_LEFT },
		{ "RIGHT", NS_KEY_RIGHT },
		{ "UP", NS_KEY_UP },
		{ "DOWN", NS_KEY_DOWN },
		{ "DEL", NS_KEY_DELETE_RIGHT },
		{ "DELETE", NS_KEY_DELETE_RIGHT },
		{ "DELETE_RIGHT", NS_KEY_DELETE_RIGHT },
		{ "HOME", NS_KEY_LINE_START },
		{ "END", NS_KEY_LINE_END },
		{ "LINE_START", NS_KEY_LINE_START },
		{ "LINE_END", NS_KEY_LINE_END },
		{ "TEXT_START", NS_KEY_TEXT_START },
		{ "TEXT_END", NS_KEY_TEXT_END },
		{ "WORD_LEFT", NS_KEY_WORD_LEFT },
		{ "DELETE_WORD_LEFT", NS_KEY_DELETE_WORD_LEFT },
		{ "WORD_RIGHT", NS_KEY_WORD_RIGHT },
		{ "DELETE_WORD_RIGHT", NS_KEY_DELETE_WORD_RIGHT },
		{ "PAGE_UP", NS_KEY_PAGE_UP },
		{ "PAGE_DOWN", NS_KEY_PAGE_DOWN },
		{ "DELETE_LINE_END", NS_KEY_DELETE_LINE_END },
		{ "DELETE_LINE_START", NS_KEY_DELETE_LINE_START },
		{ "UNDO", NS_KEY_UNDO },
		{ "REDO", NS_KEY_REDO },
		{ "SPACE", ' ' },
	};

	if (jotter_window_parse_uint32(name, key)) {
		return true;
	}

	for (size_t i = 0; i < (sizeof(key_names) / sizeof(key_names[0])); i++) {
		if (strcmp(name, key_names[i].name) == 0) {
			*key = key_names[i].key;
			return true;
		}
	}

	if (strlen(name) == 1) {
		*key = (uint8_t)name[0];
		return true;
	}

	return false;
}

static bool
jotter_window_mouse_state_from_name(const char *name, browser_mouse_state *state)
{
	struct mouse_state_name {
		const char *name;
		browser_mouse_state state;
	};
	static const struct mouse_state_name state_names[] = {
		{ "HOVER", BROWSER_MOUSE_HOVER },
		{ "PRESS_1", BROWSER_MOUSE_PRESS_1 },
		{ "PRESS_2", BROWSER_MOUSE_PRESS_2 },
		{ "PRESS_3", BROWSER_MOUSE_PRESS_3 },
		{ "PRESS_4", BROWSER_MOUSE_PRESS_4 },
		{ "PRESS_5", BROWSER_MOUSE_PRESS_5 },
		{ "CLICK_1", BROWSER_MOUSE_CLICK_1 },
		{ "CLICK_2", BROWSER_MOUSE_CLICK_2 },
		{ "CLICK_3", BROWSER_MOUSE_CLICK_3 },
		{ "CLICK_4", BROWSER_MOUSE_CLICK_4 },
		{ "CLICK_5", BROWSER_MOUSE_CLICK_5 },
		{ "DOUBLE_CLICK", BROWSER_MOUSE_DOUBLE_CLICK },
		{ "TRIPLE_CLICK", BROWSER_MOUSE_TRIPLE_CLICK },
		{ "DRAG_1", BROWSER_MOUSE_DRAG_1 },
		{ "DRAG_2", BROWSER_MOUSE_DRAG_2 },
		{ "DRAG_ON", BROWSER_MOUSE_DRAG_ON },
		{ "HOLDING_1", BROWSER_MOUSE_HOLDING_1 },
		{ "HOLDING_2", BROWSER_MOUSE_HOLDING_2 },
		{ "MOD_1", BROWSER_MOUSE_MOD_1 },
		{ "MOD_2", BROWSER_MOUSE_MOD_2 },
		{ "MOD_3", BROWSER_MOUSE_MOD_3 },
		{ "MOD_4", BROWSER_MOUSE_MOD_4 },
		{ "LEAVE", BROWSER_MOUSE_LEAVE },
	};
	uint32_t numeric;

	if (jotter_window_parse_uint32(name, &numeric)) {
		*state = (browser_mouse_state)numeric;
		return true;
	}

	for (size_t i = 0; i < (sizeof(state_names) / sizeof(state_names[0])); i++) {
		if (strcmp(name, state_names[i].name) == 0) {
			*state = state_names[i].state;
			return true;
		}
	}

	return false;
}

static bool
jotter_window_parse_mouse_state(const char *value, browser_mouse_state *state)
{
	char buffer[128];
	char *part;

	if (strlen(value) >= sizeof(buffer)) {
		return false;
	}

	strcpy(buffer, value);
	*state = BROWSER_MOUSE_HOVER;
	for (part = strtok(buffer, "+,"); part != NULL; part = strtok(NULL, "+,")) {
		browser_mouse_state part_state;

		if (!jotter_window_mouse_state_from_name(part, &part_state)) {
			return false;
		}

		*state |= part_state;
	}

	return true;
}

static void
jotter_window_handle_new(int argc, char **argv)
{
	slateurl *url = NULL;
	slateerror error = SLATEERROR_OK;

	if (argc > 3)
		return;

	if (argc == 3) {
		error = slateurl_create(argv[2], &url);
	}
	if (error == SLATEERROR_OK) {
		error = browser_window_create(BW_CREATE_HISTORY,
					      url,
					      NULL,
					      NULL,
					      NULL);
		if (url != NULL) {
			slateurl_unref(url);
		}
	}
	if (error != SLATEERROR_OK) {
		jotter_warn_user(messages_get_errorcode(error), 0);
	}
}

static void
jotter_window_handle_destroy(int argc, char **argv)
{
	struct gui_window *gw;
	uint32_t nr = atoi((argc > 2) ? argv[2] : "-1");

	gw = jotter_find_window_by_num(nr);

	if (gw == NULL) {
		moutf(MOUT_ERROR, "WINDOW NUM BAD");
	} else {
		browser_window_destroy(gw->bw);
	}
}

static void
jotter_window_handle_go(int argc, char **argv)
{
	struct gui_window *gw;
	slateurl *url;
	slateurl *ref_url = NULL;
	slateerror error;

	if (argc < 4 || argc > 5) {
		moutf(MOUT_ERROR, "WINDOW GO ARGS BAD");
		return;
	}

	gw = jotter_find_window_by_num(atoi(argv[2]));

	if (gw == NULL) {
		moutf(MOUT_ERROR, "WINDOW NUM BAD");
		return;
	}

	error = slateurl_create(argv[3], &url);
	if (error == SLATEERROR_OK) {
		if (argc == 5) {
			error = slateurl_create(argv[4], &ref_url);
		}

		if (error == SLATEERROR_OK) {
			error = browser_window_navigate(gw->bw,
							url,
							ref_url,
							BW_NAVIGATE_HISTORY,
							NULL,
							NULL,
							NULL);
			if (ref_url != NULL) {
				slateurl_unref(ref_url);
			}
		}
		slateurl_unref(url);
	}

	if (error != SLATEERROR_OK) {
		jotter_warn_user(messages_get_errorcode(error), 0);
	}
}

/**
 * handle WINDOW STOP command
 */
static void
jotter_window_handle_stop(int argc, char **argv)
{
	struct gui_window *gw;
	if (argc != 3) {
		moutf(MOUT_ERROR, "WINDOW STOP ARGS BAD\n");
		return;
	}

	gw = jotter_find_window_by_num(atoi(argv[2]));

	if (gw == NULL) {
		moutf(MOUT_ERROR, "WINDOW NUM BAD");
	} else {
		browser_window_stop(gw->bw);
	}
}


static void
jotter_window_handle_redraw(int argc, char **argv)
{
	struct gui_window *gw;
	struct rect clip;
	struct redraw_context ctx = {
		.interactive = true,
		.background_images = true,
		.plot = jotter_plotters
	};

	if (argc != 3 && argc != 7) {
		moutf(MOUT_ERROR, "WINDOW REDRAW ARGS BAD");
		return;
	}

	gw = jotter_find_window_by_num(atoi(argv[2]));

	if (gw == NULL) {
		moutf(MOUT_ERROR, "WINDOW NUM BAD");
		return;
	}

	clip.x0 = 0;
	clip.y0 = 0;
	clip.x1 = gw->width;
	clip.y1 = gw->height;

	if (argc == 7) {
		clip.x0 = atoi(argv[3]);
		clip.y0 = atoi(argv[4]);
		clip.x1 = atoi(argv[5]);
		clip.y1 = atoi(argv[6]);
	}

	NSLOG(netsurf, INFO, "Issue redraw");
	moutf(MOUT_WINDOW, "REDRAW WIN %d START", atoi(argv[2]));
	browser_window_redraw(gw->bw, gw->scrollx, gw->scrolly, &clip, &ctx);
	moutf(MOUT_WINDOW, "REDRAW WIN %d STOP", atoi(argv[2]));
}

static void
jotter_window_handle_reload(int argc, char **argv)
{
	struct gui_window *gw;
	if (argc != 3 && argc != 4) {
		moutf(MOUT_ERROR, "WINDOW RELOAD ARGS BAD\n");
		return;
	}

	gw = jotter_find_window_by_num(atoi(argv[2]));

	if (gw == NULL) {
		moutf(MOUT_ERROR, "WINDOW NUM BAD");
	} else {
		browser_window_reload(gw->bw, argc == 4);
	}
}

static void
jotter_window_handle_exec(int argc, char **argv)
{
	struct gui_window *gw;
	uint32_t win_num;

	if ((argc < 5) || (strcmp(argv[2], "WIN") != 0) ||
	    !jotter_window_parse_uint32(argv[3], &win_num)) {
		moutf(MOUT_ERROR, "WINDOW EXEC ARGS BAD\n");
		return;
	}

	gw = jotter_find_window_by_num(win_num);

	if (gw == NULL) {
		moutf(MOUT_ERROR, "WINDOW NUM BAD");
	} else {
		/* Gather argv[4] onward into a string to pass to js_exec */
		int total = 1;
		for (int i = 4; i < argc; ++i) {
			total += strlen(argv[i]) + 1;
		}
		char *cmd = calloc(total, 1);
		if (cmd == NULL) {
			moutf(MOUT_ERROR, "JS WIN %u RET ENOMEM", win_num);
			return;
		}
		char *cmdcur = cmd; /* string cursor */

		for (int argi = 4; argi < argc; ++argi) {
			int argl;
			argl = strlen(argv[argi]);
			memcpy(cmdcur, argv[argi], argl);
			cmdcur+=argl;
			*cmdcur = ' ';
			cmdcur++;
		}
		cmdcur--;
		*cmdcur = 0;

		/* Now execute the JS */

		moutf(MOUT_WINDOW, "JS WIN %u RET %s", win_num,
		      browser_window_exec(gw->bw, cmd, total - 1) ? "TRUE" : "FALSE");

		free(cmd);
	}
}

static void
jotter_window_handle_key(int argc, char **argv)
{
	struct gui_window *gw;
	uint32_t win_num;
	uint32_t key;

	/* `WINDOW KEY WIN` _%id%_ (`NAME` _%name%_ | `VALUE` _%num%_ | `TEXT` _%str...%_) */
	/*  0      1   2    3        4       5                                      */
	if ((argc < 6) || (strcmp(argv[2], "WIN") != 0) ||
	    !jotter_window_parse_uint32(argv[3], &win_num)) {
		moutf(MOUT_ERROR, "WINDOW KEY ARGS BAD\n");
		return;
	}

	gw = jotter_find_window_by_num(win_num);
	if (gw == NULL) {
		moutf(MOUT_ERROR, "WINDOW NUM BAD");
		return;
	}

	if (strcmp(argv[4], "NAME") == 0) {
		if ((argc != 6) || !jotter_window_key_from_name(argv[5], &key)) {
			moutf(MOUT_ERROR, "WINDOW KEY NAME BAD\n");
			return;
		}
		moutf(MOUT_WINDOW, "KEY WIN %u RET %s", win_num,
		      browser_window_key_press(gw->bw, key) ? "TRUE" : "FALSE");
	} else if (strcmp(argv[4], "VALUE") == 0) {
		if ((argc != 6) || !jotter_window_parse_uint32(argv[5], &key)) {
			moutf(MOUT_ERROR, "WINDOW KEY VALUE BAD\n");
			return;
		}
		moutf(MOUT_WINDOW, "KEY WIN %u RET %s", win_num,
		      browser_window_key_press(gw->bw, key) ? "TRUE" : "FALSE");
	} else if (strcmp(argv[4], "TEXT") == 0) {
		bool handled = true;

		for (int argi = 5; argi < argc; argi++) {
			if (argi > 5) {
				handled &= browser_window_key_press(gw->bw, ' ');
			}
			for (int i = 0; argv[argi][i] != '\0'; i++) {
				handled &= browser_window_key_press(
					gw->bw, (uint8_t)argv[argi][i]);
			}
		}
		moutf(MOUT_WINDOW, "KEY WIN %u RET %s", win_num,
		      handled ? "TRUE" : "FALSE");
	} else {
		moutf(MOUT_ERROR, "WINDOW KEY MODE BAD\n");
	}
}


static void
jotter_window_handle_click(int argc, char **argv)
{
	/* `WINDOW CLICK WIN` _%id%_ `X` _%num%_ `Y` _%num%_ `BUTTON` _%str%_ `KIND` _%str%_ */
	/*  0      1     2    3       4  5        6  7        8       9        10    11      */
	struct gui_window *gw;
	uint32_t win_num;

	if ((argc != 12) || (strcmp(argv[2], "WIN") != 0) ||
	    (strcmp(argv[4], "X") != 0) || (strcmp(argv[6], "Y") != 0) ||
	    (strcmp(argv[8], "BUTTON") != 0) ||
	    (strcmp(argv[10], "KIND") != 0) ||
	    !jotter_window_parse_uint32(argv[3], &win_num)) {
		moutf(MOUT_ERROR, "WINDOW CLICK ARGS BAD\n");
		return;
	}

	gw = jotter_find_window_by_num(win_num);

	if (gw == NULL) {
		moutf(MOUT_ERROR, "WINDOW NUM BAD");
	} else {
		int x = atoi(argv[5]);
		int y = atoi(argv[7]);
		browser_mouse_state mouse;
		const char *button = argv[9];
		const char *kind = argv[11];
		if (strcmp(button, "LEFT") == 0) {
			mouse = BROWSER_MOUSE_CLICK_1;
		} else if (strcmp(button, "RIGHT") == 0) {
			mouse = BROWSER_MOUSE_CLICK_2;
		} else {
			moutf(MOUT_ERROR, "WINDOW BUTTON BAD");
			return;
		}
		if (strcmp(kind, "SINGLE") == 0) {
			/* Nothing */
		} else if (strcmp(kind, "DOUBLE") == 0) {
			mouse |= BROWSER_MOUSE_DOUBLE_CLICK;
		} else if (strcmp(kind, "TRIPLE") == 0) {
			mouse |= BROWSER_MOUSE_TRIPLE_CLICK;
		} else {
			moutf(MOUT_ERROR, "WINDOW KIND BAD");
			return;
		}
		browser_window_mouse_click(gw->bw, mouse, x, y);
	}
}

static void
jotter_window_handle_mouse(int argc, char **argv)
{
	/* `WINDOW MOUSE WIN` _%id%_ `X` _%num%_ `Y` _%num%_ `STATE` _%state%_ */
	/*  0      1     2    3       4  5        6  7        8       9         */
	struct gui_window *gw;
	uint32_t win_num;
	browser_mouse_state state;

	if ((argc != 10) || (strcmp(argv[2], "WIN") != 0) ||
	    (strcmp(argv[4], "X") != 0) || (strcmp(argv[6], "Y") != 0) ||
	    (strcmp(argv[8], "STATE") != 0) ||
	    !jotter_window_parse_uint32(argv[3], &win_num)) {
		moutf(MOUT_ERROR, "WINDOW MOUSE ARGS BAD\n");
		return;
	}

	gw = jotter_find_window_by_num(win_num);
	if (gw == NULL) {
		moutf(MOUT_ERROR, "WINDOW NUM BAD");
		return;
	}

	if (!jotter_window_parse_mouse_state(argv[9], &state)) {
		moutf(MOUT_ERROR, "WINDOW MOUSE STATE BAD\n");
		return;
	}

	browser_window_mouse_track(gw->bw, state, atoi(argv[5]), atoi(argv[7]));
}

static void
jotter_window_handle_mouseclick(int argc, char **argv)
{
	/* `WINDOW MOUSECLICK WIN` _%id%_ `X` _%num%_ `Y` _%num%_ `STATE` _%state%_ */
	/*  0      1          2    3       4  5        6  7        8       9         */
	struct gui_window *gw;
	uint32_t win_num;
	browser_mouse_state state;

	if ((argc != 10) || (strcmp(argv[2], "WIN") != 0) ||
	    (strcmp(argv[4], "X") != 0) || (strcmp(argv[6], "Y") != 0) ||
	    (strcmp(argv[8], "STATE") != 0) ||
	    !jotter_window_parse_uint32(argv[3], &win_num)) {
		moutf(MOUT_ERROR, "WINDOW MOUSECLICK ARGS BAD\n");
		return;
	}

	gw = jotter_find_window_by_num(win_num);
	if (gw == NULL) {
		moutf(MOUT_ERROR, "WINDOW NUM BAD");
		return;
	}

	if (!jotter_window_parse_mouse_state(argv[9], &state)) {
		moutf(MOUT_ERROR, "WINDOW MOUSECLICK STATE BAD\n");
		return;
	}

	browser_window_mouse_click(gw->bw, state, atoi(argv[5]), atoi(argv[7]));
}

static void
jotter_window_handle_scroll(int argc, char **argv)
{
	/* `WINDOW SCROLL WIN` _%id%_ `X` _%num%_ `Y` _%num%_ `DX` _%num%_ `DY` _%num%_ */
	/*  0      1      2    3       4  5        6  7        8   9       10   11      */
	struct gui_window *gw;
	uint32_t win_num;
	bool handled;

	if ((argc != 12) || (strcmp(argv[2], "WIN") != 0) ||
	    (strcmp(argv[4], "X") != 0) || (strcmp(argv[6], "Y") != 0) ||
	    (strcmp(argv[8], "DX") != 0) || (strcmp(argv[10], "DY") != 0) ||
	    !jotter_window_parse_uint32(argv[3], &win_num)) {
		moutf(MOUT_ERROR, "WINDOW SCROLL ARGS BAD\n");
		return;
	}

	gw = jotter_find_window_by_num(win_num);
	if (gw == NULL) {
		moutf(MOUT_ERROR, "WINDOW NUM BAD");
		return;
	}

	handled = browser_window_scroll_at_point(gw->bw,
						 atoi(argv[5]),
						 atoi(argv[7]),
						 atoi(argv[9]),
						 atoi(argv[11]));
	moutf(MOUT_WINDOW, "SCROLL WIN %u RET %s",
	      win_num, handled ? "TRUE" : "FALSE");
}

void
jotter_window_handle_command(int argc, char **argv)
{
	if (argc == 1)
		return;

	if (strcmp(argv[1], "NEW") == 0) {
		jotter_window_handle_new(argc, argv);
	} else if (strcmp(argv[1], "DESTROY") == 0) {
		jotter_window_handle_destroy(argc, argv);
	} else if (strcmp(argv[1], "GO") == 0) {
		jotter_window_handle_go(argc, argv);
	} else if (strcmp(argv[1], "STOP") == 0) {
		jotter_window_handle_stop(argc, argv);
	} else if (strcmp(argv[1], "REDRAW") == 0) {
		jotter_window_handle_redraw(argc, argv);
	} else if (strcmp(argv[1], "RELOAD") == 0) {
		jotter_window_handle_reload(argc, argv);
	} else if (strcmp(argv[1], "EXEC") == 0) {
		jotter_window_handle_exec(argc, argv);
	} else if (strcmp(argv[1], "KEY") == 0) {
		jotter_window_handle_key(argc, argv);
	} else if (strcmp(argv[1], "CLICK") == 0) {
		jotter_window_handle_click(argc, argv);
	} else if (strcmp(argv[1], "MOUSE") == 0) {
		jotter_window_handle_mouse(argc, argv);
	} else if (strcmp(argv[1], "MOUSECLICK") == 0) {
		jotter_window_handle_mouseclick(argc, argv);
	} else if (strcmp(argv[1], "SCROLL") == 0) {
		jotter_window_handle_scroll(argc, argv);
	} else {
		moutf(MOUT_ERROR, "WINDOW COMMAND UNKNOWN %s\n", argv[1]);
	}

}

/**
 * process miscellaneous window events
 *
 * \param gw The window receiving the event.
 * \param event The event code.
 * \return SLATEERROR_OK when processed ok
 */
static slateerror
gui_window_event(struct gui_window *gw, enum gui_window_event event)
{
	switch (event) {
	case GW_EVENT_UPDATE_EXTENT:
		gui_window_update_extent(gw);
		break;

	case GW_EVENT_REMOVE_CARET:
		gui_window_remove_caret(gw);
		break;

	case GW_EVENT_SCROLL_START:
		gui_window_scroll_start(gw);
		break;

	case GW_EVENT_NEW_CONTENT:
		gui_window_new_content(gw);
		break;

	case GW_EVENT_START_THROBBER:
		gui_window_start_throbber(gw);
		break;

	case GW_EVENT_STOP_THROBBER:
		gui_window_stop_throbber(gw);
		break;

	case GW_EVENT_PAGE_INFO_CHANGE:
		gui_window_report_page_info(gw);
		break;

	default:
		break;
	}
	return SLATEERROR_OK;
}

static struct gui_window_table window_table = {
	.create = gui_window_create,
	.destroy = gui_window_destroy,
	.invalidate = jotter_window_invalidate_area,
	.get_scroll = gui_window_get_scroll,
	.set_scroll = gui_window_set_scroll,
	.get_dimensions = gui_window_get_dimensions,
	.event = gui_window_event,

	.set_title = gui_window_set_title,
	.set_url = gui_window_set_url,
	.set_icon = gui_window_set_icon,
	.set_status = gui_window_set_status,
	.set_pointer = gui_window_set_pointer,
	.place_caret = gui_window_place_caret,
	.drag_start = gui_window_drag_start,
	.save_link = gui_window_save_link,

	.console_log = gui_window_console_log,
};

struct gui_window_table *jotter_window_table = &window_table;
