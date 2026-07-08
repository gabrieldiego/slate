/*
 * Copyright 2012 Vincent Sanders <vince@netsurf-browser.org>
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
 * Option specific to RISC OS
 *
 * Platform specific options for RISC OS can be added by editing this file
 *
 * Global optionsshould be added in the desktop options.h.
 *
 * This header is specificaly intented to be included multiple times
 *   with different macro definitions so there is no guard
 */

#ifndef SLATE_RISCOS_OPTIONS_H_
#define SLATE_RISCOS_OPTIONS_H_

#include "riscos/tinct.h"

/* setup longer default reflow time */
#define DEFAULT_REFLOW_PERIOD 100 /* time in cs */

#define CHOICES_PREFIX "<Choices$Write>.WWW.NetSurf."

#endif

SLATEOPTION_STRING(theme, "Aletheia")
SLATEOPTION_STRING(language, NULL)
SLATEOPTION_INTEGER(plot_fg_quality, tinct_ERROR_DIFFUSE)
SLATEOPTION_INTEGER(plot_bg_quality, tinct_DITHER)
SLATEOPTION_BOOL(history_tooltip, true)
SLATEOPTION_BOOL(toolbar_show_buttons, true)
SLATEOPTION_BOOL(toolbar_show_address, true)
SLATEOPTION_BOOL(toolbar_show_throbber, true)
SLATEOPTION_STRING(toolbar_browser, "0123|58|9")
SLATEOPTION_STRING(toolbar_hotlist, "40|12|3")
SLATEOPTION_STRING(toolbar_history, "0|12|3")
SLATEOPTION_STRING(toolbar_cookies, "0|12")
SLATEOPTION_BOOL(window_stagger, true)
SLATEOPTION_BOOL(window_size_clone, true)
SLATEOPTION_BOOL(buffer_animations, true)
SLATEOPTION_BOOL(buffer_everything, true)
SLATEOPTION_BOOL(open_browser_at_startup, false)
SLATEOPTION_BOOL(no_plugins, false)
SLATEOPTION_BOOL(strip_extensions, false)
SLATEOPTION_BOOL(confirm_overwrite, true)
SLATEOPTION_BOOL(confirm_hotlist_remove, true)
SLATEOPTION_STRING(url_path, "NetSurf:URL")
SLATEOPTION_STRING(url_save, CHOICES_PREFIX "URL")
SLATEOPTION_STRING(hotlist_path, "NetSurf:Hotlist")
SLATEOPTION_STRING(hotlist_save, CHOICES_PREFIX "Hotlist")
SLATEOPTION_STRING(recent_path, "NetSurf:Recent")
SLATEOPTION_STRING(recent_save, CHOICES_PREFIX "Recent")
SLATEOPTION_STRING(theme_path, "NetSurf:Themes")
SLATEOPTION_STRING(theme_save, CHOICES_PREFIX "Themes")
SLATEOPTION_BOOL(thumbnail_iconise, true)
SLATEOPTION_BOOL(interactive_help, true)
SLATEOPTION_BOOL(external_hotlists, false)
SLATEOPTION_STRING(external_hotlist_app, NULL)

/**
 * width of screen when window_width option was saved
 */
SLATEOPTION_INTEGER(window_screen_width, 0)

/**
 * height of screen when window_heigh option was saved
 */
SLATEOPTION_INTEGER(window_screen_height, 0)
