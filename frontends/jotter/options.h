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

#ifndef SLATE_JOTTER_OPTIONS_H_
#define SLATE_JOTTER_OPTIONS_H_

/* currently nothing here */

#endif

SLATEOPTION_BOOL(downloads_clear, false)
SLATEOPTION_BOOL(request_overwrite, true)
SLATEOPTION_STRING(downloads_directory, NULL)
SLATEOPTION_STRING(url_file, NULL)
SLATEOPTION_BOOL(show_single_tab, false)
SLATEOPTION_INTEGER(button_type, 0)
SLATEOPTION_INTEGER(history_age, 0)
SLATEOPTION_BOOL(hover_urls, false)
SLATEOPTION_BOOL(new_blank, false)
SLATEOPTION_STRING(hotlist_path, NULL)
SLATEOPTION_BOOL(source_tab, false)
SLATEOPTION_INTEGER(current_theme, 0)


