/*
 * Copyright 2021 Vincent Sanders <vince@netsurf-browser.org>
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

#ifndef SLATE_QT_LAYOUT_H
#define SLATE_QT_LAYOUT_H 1

/**
 * qt layout operations table
 */
extern struct gui_layout_table *slateqt_layout_table;


/**
 * Text plotting.
 *
 * \param painter The QT painter to use
 * \param fstyle plot style for this text
 * \param x x coordinate
 * \param y y coordinate
 * \param text UTF-8 string to plot
 * \param length length of string, in bytes
 * \return SLATEERROR_OK on success else error code.
 */
slateerror slateqt_layout_plot(QPainter* painter, const struct plot_font_style *fstyle, int x, int y, const char *text, size_t length);

#endif
