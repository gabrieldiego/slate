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

/** \file
 * Low-level resource cache null persistant storage implementation.
 */

#include "utils/slateurl.h"

#include "content/backing_store.h"


/* default to disabled backing store */
static slateerror initialise(const struct llcache_store_parameters *parameters)
{
	return SLATEERROR_OK;
}

static slateerror finalise(void)
{
	return SLATEERROR_OK;
}

static slateerror store(slateurl *url,
		     enum backing_store_flags flags,
		     uint8_t *data,
		     const size_t datalen)
{
	return SLATEERROR_SAVE_FAILED;
}

static slateerror fetch(slateurl *url,
		     enum backing_store_flags flags,
		     uint8_t **data_out,
		     size_t *datalen_out)
{
	return SLATEERROR_NOT_FOUND;
}

static slateerror invalidate(slateurl *url)
{
	return SLATEERROR_NOT_FOUND;
}

/**
 * release a previously fetched or stored memory object.
 *
 * if the BACKING_STORE_ALLOC flag was used with the fetch or
 * store operation for this url the returned storage is
 * unreferenced. When the reference count drops to zero the
 * storage is released.
 *
 * @param url The url is used as the unique primary key to invalidate.
 * @param[in] flags The flags to control how the object data is released.
 * @return SLATEERROR_OK on success or error code on faliure.
 */
static slateerror release(slateurl *url, enum backing_store_flags flags)
{
	return SLATEERROR_NOT_FOUND;
}

static struct gui_llcache_table llcache_table = {
	.initialise = initialise,
	.finalise = finalise,
	.store = store,
	.fetch = fetch,
	.invalidate = invalidate,
	.release = release,
};

struct gui_llcache_table *null_llcache_table = &llcache_table;
