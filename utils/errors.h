/*
 * Copyright 2009 John-Mark Bell <jmb@netsurf-browser.org>
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

/** \file
 * Error codes
 */

#ifndef SLATE_UTILS_ERRORS_H_
#define SLATE_UTILS_ERRORS_H_

/**
 * Enumeration of error codes
 */
typedef enum {
	SLATEERROR_OK,                     /**< No error */
	SLATEERROR_UNKNOWN,                /**< Unknown error - DO *NOT* USE */
	SLATEERROR_NOMEM,                  /**< Memory exhaustion */
	SLATEERROR_NO_FETCH_HANDLER,       /**< No fetch handler for URL scheme */
	SLATEERROR_NOT_FOUND,              /**< Requested item not found */
	SLATEERROR_NOT_DIRECTORY,          /**< Missing directory */
	SLATEERROR_SAVE_FAILED,            /**< Failed to save data */
	SLATEERROR_CLONE_FAILED,           /**< Failed to clone handle */
	SLATEERROR_INIT_FAILED,            /**< Initialisation failed */
	SLATEERROR_BMP_ERROR,              /**< A BMP error occurred */
	SLATEERROR_GIF_ERROR,              /**< A GIF error occurred */
	SLATEERROR_ICO_ERROR,              /**< A ICO error occurred */
	SLATEERROR_PNG_ERROR,              /**< A PNG error occurred */
	SLATEERROR_SPRITE_ERROR,           /**< A RISC OS Sprite error occurred */
	SLATEERROR_SVG_ERROR,              /**< A SVG error occurred */
	SLATEERROR_BAD_ENCODING,           /**< The character set is unknown */
	SLATEERROR_NEED_DATA,              /**< More data needed */
	SLATEERROR_ENCODING_CHANGE,        /**< The character changed */
	SLATEERROR_BAD_PARAMETER,          /**< Bad Parameter */
	SLATEERROR_INVALID,                /**< Invalid data */
	SLATEERROR_BOX_CONVERT,            /**< Box conversion failed */
	SLATEERROR_STOPPED,                /**< Content conversion stopped */
	SLATEERROR_DOM,                    /**< DOM call returned error */
	SLATEERROR_CSS,                    /**< CSS call returned error */
	SLATEERROR_CSS_BASE,               /**< CSS base sheet failed */
	SLATEERROR_BAD_URL,                /**< Bad URL */
	SLATEERROR_BAD_CONTENT,            /**< Bad Content */
	SLATEERROR_FRAME_DEPTH,            /**< Exceeded frame depth */
	SLATEERROR_PERMISSION,             /**< Permission error */
	SLATEERROR_NOSPACE,                /**< Insufficient space */
	SLATEERROR_BAD_SIZE,               /**< Bad size */
	SLATEERROR_NOT_IMPLEMENTED,        /**< Functionality is not implemented */
	SLATEERROR_BAD_REDIRECT,           /**< Fetch encountered a bad redirect */
	SLATEERROR_CYCLIC_REDIRECT,        /**< Fetch encountered a cyclic redirect */
	SLATEERROR_UNSAFE_REDIRECT,        /**< Fetch encountered a unsafe redirect */
	SLATEERROR_BAD_AUTH,               /**< Fetch needs authentication data */
	SLATEERROR_BAD_CERTS,              /**< Fetch needs certificate chain check */
	SLATEERROR_TIMEOUT,                /**< Operation timed out */
} slateerror;

#endif

