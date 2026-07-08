/*
 * Copyright 2011 Michael Drake <tlsa@netsurf-browser.org>
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
 * NetSurf URL handling (interface).
 */

#ifndef _SLATE_UTILS_SLATEURL_H_
#define _SLATE_UTILS_SLATEURL_H_

#include <libwapcaplet/libwapcaplet.h>
#include "utils/errors.h"


/** NetSurf URL object */
typedef struct slateurl slateurl;

/** A type for URL schemes */
enum slateurl_scheme_type {
	SLATEURL_SCHEME_OTHER,
	SLATEURL_SCHEME_HTTP,
	SLATEURL_SCHEME_HTTPS,
	SLATEURL_SCHEME_FILE,
	SLATEURL_SCHEME_FTP,
	SLATEURL_SCHEME_MAILTO,
	SLATEURL_SCHEME_DATA
};

typedef enum slateurl_component {
	SLATEURL_SCHEME		= (1 << 0),
	SLATEURL_USERNAME		= (1 << 1),
	SLATEURL_PASSWORD		= (1 << 2),
	SLATEURL_CREDENTIALS	= SLATEURL_USERNAME | SLATEURL_PASSWORD,
	SLATEURL_HOST		= (1 << 3),
	SLATEURL_PORT		= (1 << 4),
	SLATEURL_AUTHORITY		= SLATEURL_CREDENTIALS | SLATEURL_HOST | SLATEURL_PORT,
	SLATEURL_PATH		= (1 << 5),
	SLATEURL_QUERY		= (1 << 6),
	SLATEURL_COMPLETE		= SLATEURL_SCHEME | SLATEURL_AUTHORITY |
				  SLATEURL_PATH | SLATEURL_QUERY,
	SLATEURL_FRAGMENT		= (1 << 7),
	SLATEURL_WITH_FRAGMENT	= SLATEURL_COMPLETE | SLATEURL_FRAGMENT
} slateurl_component;


/**
 * Create a NetSurf URL object from a URL string
 *
 * \param url_s	  String to create NetSurf URL from
 * \param url	  Returns a NetSurf URL
 * \return SLATEERROR_OK on success, appropriate error otherwise
 *
 * If return value != SLATEERROR_OK, nothing will be returned in url.
 *
 * It is up to the client to call slateurl_unref when they are finished with
 * the created object.
 */
slateerror slateurl_create(const char * const url_s, slateurl **url);


/**
 * Increment the reference count to a NetSurf URL object
 *
 * \param url	  NetSurf URL to create another reference to
 * \return The NetSurf URL pointer to use as the copy
 *
 * Use this when copying a NetSurf URL into a persistent data structure.
 */
slateurl *slateurl_ref(slateurl *url);


/**
 * Drop a reference to a NetSurf URL object
 *
 * \param url	  NetSurf URL to drop reference to
 *
 * When the reference count reaches zero then the NetSurf URL will be destroyed
 */
void slateurl_unref(slateurl *url);


/**
 * Compare two URLs
 *
 * \param url1	  First NetSurf URL
 * \param url2	  Second NetSurf URL
 * \param parts	  The URL components to be compared
 * \return true on match else false
 *
 */
bool slateurl_compare(const slateurl *url1, const slateurl *url2, slateurl_component parts);


/**
 * Get URL (section) as a string, from a NetSurf URL object
 *
 * \param url	  NetSurf URL
 * \param parts	  The required URL components.
 * \param url_s	  Returns a url string
 * \param url_l	  Returns length of url_s
 * \return SLATEERROR_OK on success, appropriate error otherwise
 *
 * If return value != SLATEERROR_OK, nothing will be returned in url_s or url_l.
 *
 * The string returned in url_s is owned by the client and it is up to them
 * to free it.  It includes a trailing '\0'.
 *
 * The length returned in url_l excludes the trailing '\0'.
 *
 * That the required URL components be consecutive is not enforced, however,
 * non-consecutive URL components generally make no sense.  The exception
 * is removal of credentials from a URL, such as for display in browser
 * window URL bar.  'SLATEURL_COMPLETE &~ SLATEURL_PASSWORD' would remove the
 * password from a complete URL.
 */
slateerror slateurl_get(const slateurl *url, slateurl_component parts,
		char **url_s, size_t *url_l);


/**
 * Get part of a URL as a lwc_string, from a NetSurf URL object
 *
 * \param url	  NetSurf URL object
 * \param part	  The URL component required
 * \return the required component as an lwc_string, or NULL
 *
 * The caller owns the returned lwc_string and should call lwc_string_unref
 * when they are done with it.
 *
 * The valid values for the part parameter are:
 *    SLATEURL_SCHEME
 *    SLATEURL_USERNAME
 *    SLATEURL_PASSWORD
 *    SLATEURL_HOST
 *    SLATEURL_PORT
 *    SLATEURL_PATH
 *    SLATEURL_QUERY
 *    SLATEURL_FRAGMENT
 */
lwc_string *slateurl_get_component(const slateurl *url, slateurl_component part);


/**
 * Get the scheme type from a NetSurf URL object
 *
 * \param url   NetSurf URL object
 * \return The URL scheme type.
 */
enum slateurl_scheme_type slateurl_get_scheme_type(const slateurl *url);


/**
 * Enquire about the existence of componenets in a given URL
 *
 * \param url	  NetSurf URL object
 * \param part	  The URL components confirm existence of
 * \return true iff the component in question exists in url
 *
 * The valid values for the part parameter are:
 *    SLATEURL_SCHEME
 *    SLATEURL_USERNAME
 *    SLATEURL_PASSWORD
 *    SLATEURL_CREDENTIALS
 *    SLATEURL_HOST
 *    SLATEURL_PORT
 *    SLATEURL_PATH
 *    SLATEURL_QUERY
 *    SLATEURL_FRAGMENT
 */
bool slateurl_has_component(const slateurl *url, slateurl_component part);


/**
 * Access a NetSurf URL object as a string
 *
 * \param url	  NetSurf URL to retrieve a string pointer for.
 * \return the required string
 *
 * The returned string is owned by the NetSurf URL object.  It will die
 * with the NetSurf URL object.  Keep a reference to the URL if you need it.
 *
 * The returned string has a trailing '\0'.
 */
const char *slateurl_access(const slateurl *url);


/**
 * Variant of \ref slateurl_access for logging.
 *
 * \param url	  NetSurf URL to retrieve a string pointer for.
 * \return the required string
 *
 * This will not necessarily return the actual slateurl's URL, but something
 * that is suitable for recording to logs.  E.g. URLs with the `data` scheme
 * will return a simple place holder, to avoid repeatedly dumping loads of data.
 *
 * The returned string is owned by the NetSurf URL object.  It will die
 * with the NetSurf URL object.  Keep a reference to the URL if you need it.
 *
 * The returned string has a trailing '\0'.
 */
const char *slateurl_access_log(const slateurl *url);


/**
 * Get a UTF-8 string (for human readable IDNs) from a NetSurf URL object
 *
 * \param url	  NetSurf URL object
 * \param url_s	  Returns a url string
 * \param url_l	  Returns length of url_s
 * \return SLATEERROR_OK on success, appropriate error otherwise
 *
 * If return value != SLATEERROR_OK, nothing will be returned in url_s or url_l.
 *
 * The string returned in url_s is owned by the client and it is up to them
 * to free it.  It includes a trailing '\0'.
 *
 * The length returned in url_l excludes the trailing '\0'.
 */
slateerror slateurl_get_utf8(const slateurl *url, char **url_s, size_t *url_l);


/**
 * Access a URL's path leaf as a string
 *
 * \param url	  NetSurf URL to retrieve a string pointer for.
 * \return the required string
 *
 * The returned string is owned by the NetSurf URL object.  It will die
 * with the NetSurf URL object.  Keep a reference to the URL if you need it.
 *
 * The returned string has a trailing '\0'.
 */
const char *slateurl_access_leaf(const slateurl *url);


/**
 * Find the length of a NetSurf URL object's URL, as returned by slateurl_access
 *
 * \param url	  NetSurf URL to find length of.
 * \return the required string
 *
 * The returned length excludes the trailing '\0'.
 */
size_t slateurl_length(const slateurl *url);


/**
 * Get a URL's hash value
 *
 * \param url	  NetSurf URL get hash value for.
 * \return the hash value
 */
uint32_t slateurl_hash(const slateurl *url);


/**
 * Join a base url to a relative link part, creating a new NetSurf URL object
 *
 * \param base	  NetSurf URL containing the base to join rel to
 * \param rel	  String containing the relative link part
 * \param joined  Returns joined NetSurf URL
 * \return SLATEERROR_OK on success, appropriate error otherwise
 *
 * If return value != SLATEERROR_OK, nothing will be returned in join.
 *
 * It is up to the client to call slateurl_unref when they are finished with
 * the created object.
 */
slateerror slateurl_join(const slateurl *base, const char *rel, slateurl **joined);


/**
 * Create a NetSurf URL object without a fragment from a NetSurf URL
 *
 * \param url	  NetSurf URL to create new NetSurf URL from
 * \param no_frag Returns new NetSurf URL without fragment
 * \return SLATEERROR_OK on success, appropriate error otherwise
 *
 * If return value != SLATEERROR_OK, nothing will be returned in no_frag.
 *
 * It is up to the client to call slateurl_unref when they are finished with
 * the created object.
 */
slateerror slateurl_defragment(const slateurl *url, slateurl **no_frag);


/**
 * Create a NetSurf URL object, adding a fragment to an existing URL object
 *
 * \param url	  NetSurf URL to create new NetSurf URL from
 * \param frag	  Fragment to add
 * \param new_url Returns new NetSurf URL without fragment
 * \return SLATEERROR_OK on success, appropriate error otherwise
 *
 * If return value != SLATEERROR_OK, nothing will be returned in new_url.
 *
 * It is up to the client to call slateurl_unref when they are finished with
 * the created object.
 *
 * Any fragment in url is replaced with frag in new_url.
 */
slateerror slateurl_refragment(const slateurl *url, lwc_string *frag, slateurl **new_url);


/**
 * Create a NetSurf URL object, with query string replaced
 *
 * \param url	  NetSurf URL to create new NetSurf URL from
 * \param query	  Query string to use
 * \param new_url Returns new NetSurf URL with query string provided
 * \return SLATEERROR_OK on success, appropriate error otherwise
 *
 * If return value != SLATEERROR_OK, nothing will be returned in new_url.
 *
 * It is up to the client to call slateurl_unref when they are finished with
 * the created object.
 *
 * Any query component in url is replaced with query in new_url.
 *
 * Passing the empty string as a replacement will result in the query
 * component being removed.
 */
slateerror slateurl_replace_query(const slateurl *url, const char *query,
		slateurl **new_url);


/**
 * Create a NetSurf URL object, with scheme replaced
 *
 * \param url	  NetSurf URL to create new NetSurf URL from
 * \param scheme  Scheme to use
 * \param new_url Returns new NetSurf URL with scheme provided
 * \return SLATEERROR_OK on success, appropriate error otherwise
 *
 * If return value != SLATEERROR_OK, nothing will be returned in new_url.
 *
 * It is up to the client to call slateurl_unref when they are finished with
 * the created object.
 *
 * Any scheme component in url is replaced with scheme in new_url.
 */
slateerror slateurl_replace_scheme(const slateurl *url, lwc_string *scheme,
		slateurl **new_url);


/**
 * Attempt to find a nice filename for a URL.
 *
 * \param url		A NetSurf URL object to create a filename from
 * \param result	Updated to caller-owned string with filename
 * \param remove_extensions  remove any extensions from the filename
 * \return SLATEERROR_OK on success, appropriate error otherwise
 *
 * Caller must ensure string result string is freed, if SLATEERROR_OK returned.
 */
slateerror slateurl_nice(const slateurl *url, char **result, bool remove_extensions);


/**
 * Create a NetSurf URL object for URL with parent location of an existing URL.
 *
 * \param url	  NetSurf URL to create new NetSurf URL from
 * \param new_url Returns new NetSurf URL with parent URL path
 * \return SLATEERROR_OK on success, appropriate error otherwise
 *
 * If return value != SLATEERROR_OK, nothing will be returned in new_url.
 *
 * It is up to the client to call slateurl_unref when they are finished with
 * the created object.
 *
 * As well as stripping top most path segment, query and fragments are stripped.
 */
slateerror slateurl_parent(const slateurl *url, slateurl **new_url);

/**
 * Dump a NetSurf URL's internal components to stderr
 *
 * This is helper functionality for developers, and shouldn't be called
 * generally.
 *
 * \param url	The NetSurf URL to dump components of
 */
void slateurl_dump(const slateurl *url);

#endif
