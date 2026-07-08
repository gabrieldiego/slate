/*
 * Copyright 2006 John M Bell <jmb202@ecs.soton.ac.uk>
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
 * Unified URL information database internal interface.
 */

#ifndef SLATE_CONTENT_URLDB_H
#define SLATE_CONTENT_URLDB_H

#include <libwapcaplet/libwapcaplet.h>

#include "slate/url_db.h"
#include "slate/cookie_db.h"

/**
 * Destroy urldb
 */
void urldb_destroy(void);


/**
 * Set the cross-session persistence of the entry for an URL
 *
 * \param url Absolute URL to persist
 * \param persist True to persist, false otherwise
 * \return SLATEERROR_OK on success or SLATEERROR_NOT_FOUND if url not in database
 */
slateerror urldb_set_url_persistence(struct slateurl *url, bool persist);


/**
 * Insert an URL into the database
 *
 * \param url Absolute URL to insert
 * \return true on success, false otherwise
 */
bool urldb_add_url(struct slateurl *url);


/**
 * Set an URL's title string, replacing any existing one
 *
 * \param url The URL to look for
 * \param title The title string to use (copied)
 * \return SLATEERROR_OK on success otherwise appropriate error code
 */
slateerror urldb_set_url_title(struct slateurl *url, const char *title);


/**
 * Set an URL's content type
 *
 * \param url The URL to look for
 * \param type The type to set
 * \return SLATEERROR_OK on success or SLATEERROR_NOT_FOUND if url not in database
 */
slateerror urldb_set_url_content_type(struct slateurl *url, content_type type);


/**
 * Set authentication data for an URL
 *
 * \param url The URL to consider
 * \param realm The authentication realm
 * \param auth The authentication details (in form username:password)
 */
void urldb_set_auth_details(struct slateurl *url, const char *realm, const char *auth);


/**
 * Look up authentication details in database
 *
 * \param url Absolute URL to search for
 * \param realm When non-NULL, it is realm which can be used to determine
 *        the protection space when that's not been done before for given URL.
 * \return Pointer to authentication details, or NULL if not found
 */
const char *urldb_get_auth_details(struct slateurl *url, const char *realm);


/**
 * Update an URL's visit data
 *
 * \param url The URL to update
 * \return SLATEERROR_OK on success or SLATEERROR_NOT_FOUND if url not in database
 */
slateerror urldb_update_url_visit_data(struct slateurl *url);


/**
 * Reset an URL's visit statistics
 *
 * \param url The URL to reset
 */
void urldb_reset_url_visit_data(struct slateurl *url);


/**
 * Extract an URL from the db
 *
 * \param url URL to extract
 * \return Pointer to database's copy of URL or NULL if not found
 */
struct slateurl *urldb_get_url(struct slateurl *url);


/**
 * Retrieve certificate verification permissions from database
 *
 * \param url Absolute URL to search for
 * \return true to permit connections to hosts with invalid certificates,
 * false otherwise.
 */
bool urldb_get_cert_permissions(struct slateurl *url);


/**
 * Parse Set-Cookie header and insert cookie(s) into database
 *
 * \param header Header to parse, with Set-Cookie: stripped
 * \param url URL being fetched
 * \param referrer Referring resource, or 0 for verifiable transaction
 * \return true on success, false otherwise
 */
bool urldb_set_cookie(const char *header, struct slateurl *url, struct slateurl *referrer);


/**
 * Retrieve cookies for an URL
 *
 * \param url URL being fetched
 * \param include_http_only Whether to include HTTP(S) only cookies.
 * \return Cookies string for libcurl (on heap), or NULL on error/no cookies
 */
char *urldb_get_cookie(struct slateurl *url, bool include_http_only);


/**
 * Set HSTS policy for an URL
 *
 * \param url URL being fetched
 * \param header Strict-Transport-Security header value
 * \return true on success, false otherwise
 */
bool urldb_set_hsts_policy(struct slateurl *url, const char *header);


/**
 * Determine if HSTS policy is enabled for an URL
 *
 * \param url URL being fetched
 * \return true if HSTS policy is enabled, false otherwise
 */
bool urldb_get_hsts_enabled(struct slateurl *url);

#endif
