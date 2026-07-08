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

#ifndef SLATE_QT_RESOURCES_H
#define SLATE_QT_RESOURCES_H 1

/**
 * resource search path vector
 */
extern char **respaths;

/**
 * Create an array of valid paths to search for resources.
 *
 * The idea is that all the complex path computation to find resources
 * is performed here, once, rather than every time a resource is
 * searched for.
 *
 * \param resource_path A shell style colon separated path list

 * \return SLATEERROR_OK on success and the respaths set to a string
 *         vector of valid paths where resources can be found or appropriate
 *         error code on faliure.
 */
slateerror slateqt_init_resource_path(const char *resource_path);

#endif
