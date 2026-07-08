/*
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
 * Interface to win32 global history manager using slatew32 core window
 */

#ifndef SLATE_WINDOWS_GLOBAL_HISTORY_H
#define SLATE_WINDOWS_GLOBAL_HISTORY_H

/**
 * make the global history window visible.
 *
 * \return SLATEERROR_OK on success else appropriate error code on faliure.
 */
slateerror slatew32_global_history_present(HINSTANCE hinstance);

/**
 * Destroys the global history window and performs any other necessary cleanup
 * actions.
 */
slateerror slatew32_global_history_finalise(void);

#endif
