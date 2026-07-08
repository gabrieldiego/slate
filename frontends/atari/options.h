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

#ifndef NS_ATARI_OPTIONS_H_
#define NS_ATARI_OPTIONS_H_

/* setup longer default reflow time */
#define DEFAULT_REFLOW_PERIOD 350 /* time in cs */

#endif

SLATEOPTION_STRING(atari_font_driver, "freetype")
SLATEOPTION_INTEGER(atari_font_monochrom, 0)
SLATEOPTION_INTEGER(atari_transparency, 1)
SLATEOPTION_INTEGER(atari_dither, 1)
SLATEOPTION_INTEGER(atari_gui_poll_timeout, 0)
SLATEOPTION_STRING(atari_editor, NULL)
SLATEOPTION_STRING(font_face_sans_serif, NULL)
SLATEOPTION_STRING(font_face_sans_serif_bold, NULL)
SLATEOPTION_STRING(font_face_sans_serif_italic, NULL)
SLATEOPTION_STRING(font_face_sans_serif_italic_bold, NULL)
SLATEOPTION_STRING(font_face_monospace, NULL)
SLATEOPTION_STRING(font_face_monospace_bold, NULL)
SLATEOPTION_STRING(font_face_serif, NULL)
SLATEOPTION_STRING(font_face_serif_bold, NULL)
SLATEOPTION_STRING(font_face_cursive, NULL)
SLATEOPTION_STRING(font_face_fantasy, NULL)
SLATEOPTION_STRING(downloads_path, "downloads")
SLATEOPTION_STRING(url_file, "url.db")
SLATEOPTION_STRING(hotlist_file, "hotlist")
SLATEOPTION_STRING(tree_icons_path, "./res/icons")


