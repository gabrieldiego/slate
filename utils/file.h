/*
 * Copyright 2014 Vincent Sanders <vince@netsurf-browser.org>
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
 * \file utils/file.h
 * \brief Default operations table for files.
 *
 * These are file operations that depend upon the filesystem the
 * browser is operating on. These allow the core browser functionality
 * to be filesystem agnostic.
 *
 * The provided defaults operate on POSIX path names with / as a
 * directory separator in a single hieracy from a root directory.
 *
 * Other path conventions require the default operations to be
 * overridden. For example windows frontend runs on a filesystem with
 * drive letter and a \\ as a separator.
 */

#ifndef SLATE_UTILS_FILE_H
#define SLATE_UTILS_FILE_H

#include <stdarg.h>

struct slateurl;

/**
 * /brief function table for file and filename operations.
 *
 * function table implementing an interface to file and filename
 * functionality appropriate for the OS filesystem. All paths are in
 * terms of the OS filesystem convention and must not require additional
 * system specific changes.
 */
struct gui_file_table {
	/* Mandantory entries */

	/**
	 * Generate a path from one or more component elemnts.
	 *
	 * If a string is allocated it must be freed by the caller.
	 *
	 * @param[in,out] str pointer to string pointer if this is NULL enough
	 *                    storage will be allocated for the complete path.
	 * @param[in,out] size The size of the space available if \a str not
	 *                     NULL on input and if not NULL set to the total
	 *                     output length on output.
	 * @param[in] nemb The number of elements.
	 * @param[in] ap The elements of the path as string pointers.
	 * @return SLATEERROR_OK and the complete path is written to str
	 *         or error code on faliure.
	 */
	slateerror (*mkpath)(char **str, size_t *size, size_t nemb, va_list ap);

	/**
	 * Get the basename of a file.
	 *
	 * This gets the last element of a path and returns it.
	 *
	 * @param[in] path The path to extract the name from.
	 * @param[in,out] str Pointer to string pointer if this is NULL enough
	 *                    storage will be allocated for the path element.
	 * @param[in,out] size The size of the space available if \a
	 *                     str not NULL on input and set to the total
	 *                     output length on output.
	 * @return SLATEERROR_OK and the complete path is written to \a str
	 *         or error code on faliure.
	 */
	slateerror (*basename)(const char *path, char **str, size_t *size);

	/**
	 * Create a path from a slateurl.
	 *
	 * @param[in] url The url to encode.
	 * @param[out] path A string containing the result path which
	 *                  must be freed by the caller.
	 * @return SLATEERROR_OK and the path is written to \a path
	 *         or error code on faliure.
	 */
	slateerror (*slateurl_to_path)(struct slateurl *url, char **path);

	/**
	 * Create a slateurl from a path.
	 *
	 * Perform the necessary operations on a path to generate a slateurl.
	 *
	 * @param[in] path The path to convert.
	 * @param[out] url pointer to recive the slateurl, The returned
	 *                 url should be unreferenced by the caller.
	 * @return SLATEERROR_OK and the url is placed in \a url or error
	 *         code on faliure.
	 */
	slateerror (*path_to_slateurl)(const char *path, struct slateurl **url);

	/**
	 * Ensure that all directory elements needed to store a filename exist.
	 *
	 * @param[in] fname The filename to ensure the path to exists.
	 * @return SLATEERROR_OK on success or error code on failure.
	 */
	slateerror (*mkdir_all)(const char *fname);
};

/** Default (posix) file operation table. */
extern struct gui_file_table *default_file_table;

/**
 * Generate a path from one or more component elemnts.
 *
 * If a string is allocated it must be freed by the caller.
 *
 * @warning If this is called before the gui operation tables are
 * initialised the behaviour defaults to posix paths. Ensure this is
 * the required behaviour.
 *
 * @param[in,out] str pointer to string pointer if this is NULL enough
 *                    storage will be allocated for the complete path.
 * @param[in,out] size The size of the space available if \a str not
 *                     NULL on input and if not NULL set to the total
 *                     output length on output.
 * @param[in] nelm The number of elements.
 * @param[in] ... The elements of the path as string pointers.
 * @return SLATEERROR_OK and the complete path is written to str
 *         or error code on faliure.
 */
slateerror slate_mkpath(char **str, size_t *size, size_t nelm, ...);

/**
 * Create a path from a slateurl.
 *
 * @param[in] url The url to encode.
 * @param[out] path_out A string containing the result path which  must be
 *                      freed by the caller.
 * @return SLATEERROR_OK and the path is written to \a path_out or error code on
 *         faliure.
 */
slateerror slate_slateurl_to_path(struct slateurl *url, char **path_out);

/**
 * Create a slateurl from a path.
 *
 * Perform the necessary operations on a path to generate a slateurl.
 *
 * @param[in] path The path to convert.
 * @param[out] url pointer to recive the slateurl, The returned
 *                 url should be unreferenced by the caller.
 * @return SLATEERROR_OK and the url is placed in \a url or error
 *         code on faliure.
 */
slateerror slate_path_to_slateurl(const char *path, struct slateurl **url);

/**
 * Ensure that all directory elements needed to store a filename exist.
 *
 * @param fname The filename to ensure the path to exists.
 * @return SLATEERROR_OK on success or error code on failure.
 */
slateerror slate_mkdir_all(const char *fname);

/**
 * Recursively remove a directory
 *
 * If this returns a failure code, there's an unpredictable amount left
 * unremoved.
 *
 * @param path The path to recursively remove
 * @return SLATEERROR_OK on success, or an error code on failure.
 */
slateerror slate_recursive_rm(const char *path);

#endif
