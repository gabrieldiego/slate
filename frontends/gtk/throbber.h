/*
 * Copyright 2008 Rob Kendrick <rjek@netsurf-browser.org>
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

#ifndef SLATE_GTK_THROBBER_H
#define SLATE_GTK_THROBBER_H

/**
 * Initialise global throbber context
 */
slateerror slategtk_throbber_init(void);

/**
 * release global throbber context
 */
void slategtk_throbber_finalise(void);

/**
 * get the pixbuf of a given frame of the throbber
 *
 * \param frame The frame number starting at 0 for stopped frame
 * \param pixbuf updated on success
 * \return SLATEERROR_OK and pixbuf updated on success, SLATEERROR_BAD_SIZE if frame
 *          is out of range else error code.
 */
slateerror slategtk_throbber_get_frame(int frame, GdkPixbuf **pixbuf);

#endif /* SLATE_GTK_THROBBER_H */
