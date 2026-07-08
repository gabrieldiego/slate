/*
 * Copyright 2008 - 2025 Chris Young <chris@unsatisfactorysoftware.co.uk>
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

#ifndef AMIGA_OPTIONS_H
#define AMIGA_OPTIONS_H

/* currently nothing here */

#endif



SLATEOPTION_STRING(url_file, NULL)
SLATEOPTION_STRING(hotlist_file, NULL)
SLATEOPTION_STRING(pubscreen_name, NULL)
SLATEOPTION_STRING(screen_modeid, NULL)
SLATEOPTION_INTEGER(screen_compositing, -1)
SLATEOPTION_INTEGER(screen_ydpi, 85)
SLATEOPTION_INTEGER(cache_bitmaps, 0)
SLATEOPTION_STRING(theme, "PROGDIR:Resources/Themes/Default")
SLATEOPTION_BOOL(clipboard_write_utf8, false)
SLATEOPTION_BOOL(truecolour_mouse_pointers, false)
SLATEOPTION_BOOL(os_mouse_pointers, true)
SLATEOPTION_BOOL(use_openurl_lib, false)
SLATEOPTION_BOOL(tab_close_warn, true)
SLATEOPTION_BOOL(tab_always_show, false)
SLATEOPTION_BOOL(tab_new_session, false) /* When NetSurf is already running, open new tab */
SLATEOPTION_BOOL(kiosk_mode, false)
SLATEOPTION_STRING(search_engines_file, "PROGDIR:Resources/SearchEngines")
SLATEOPTION_STRING(arexx_dir, "PROGDIR:Rexx")
SLATEOPTION_STRING(arexx_startup, "Startup.nsrx")
SLATEOPTION_STRING(arexx_shutdown, "Shutdown.nsrx")
SLATEOPTION_BOOL(arexx_allow_exec, false)
SLATEOPTION_STRING(download_dir, NULL)
SLATEOPTION_BOOL(download_notify, true)
SLATEOPTION_BOOL(download_notify_progress, false)
SLATEOPTION_BOOL(faster_scroll, true)
SLATEOPTION_BOOL(scale_quality, false)
SLATEOPTION_INTEGER(dither_quality, 0)
SLATEOPTION_INTEGER(mask_alpha, 0)
SLATEOPTION_BOOL(ask_overwrite, true)
SLATEOPTION_INTEGER(printer_unit, 0)
SLATEOPTION_INTEGER(print_scale, 100)
SLATEOPTION_BOOL(startup_no_window, false)
SLATEOPTION_BOOL(close_no_quit, false)
SLATEOPTION_BOOL(hide_docky_icon, false)
SLATEOPTION_STRING(font_unicode, NULL)
SLATEOPTION_STRING(font_surrogate, NULL)
SLATEOPTION_STRING(font_unicode_file, NULL)
SLATEOPTION_BOOL(font_unicode_only, false)
SLATEOPTION_BOOL(font_antialiasing, true)
SLATEOPTION_BOOL(bitmap_fonts, false)
SLATEOPTION_BOOL(drag_save_icons, true)
SLATEOPTION_INTEGER(web_search_width, 0)
SLATEOPTION_BOOL(window_simple_refresh, true)
SLATEOPTION_BOOL(resize_with_contents, false)
SLATEOPTION_INTEGER(reformat_delay, 0)
SLATEOPTION_INTEGER(redraw_tile_size_x, 0)
SLATEOPTION_INTEGER(redraw_tile_size_y, 0)
SLATEOPTION_INTEGER(monitor_aspect_x, 0)
SLATEOPTION_INTEGER(monitor_aspect_y, 0)
SLATEOPTION_BOOL(accept_lang_locale, true)

/* Local charset when using iconv */
SLATEOPTION_STRING(local_charset, "ISO-8859-1")

#ifdef __amigaos4__
/** Options relevant for OS4 only **/

/* Local charset IANA number when using codesets */
SLATEOPTION_INTEGER(local_codeset, 0)

/* Use ExtMem */
SLATEOPTION_BOOL(use_extmem, true)

/* Don't invert alpha channel (guigfx) */
SLATEOPTION_BOOL(invert_alpha, false)

#else
/** Options relevant for OS3 only **/

SLATEOPTION_BOOL(friend_bitmap, false)

/* Invert alpha channel (guigfx)
 * Workaround for guigfx/render bug
 */
SLATEOPTION_BOOL(invert_alpha, true)

#endif

