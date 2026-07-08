/*
 * Copyright 2010 John-Mark Bell <jmb@netsurf-browser.org>
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

#include <stdlib.h>
#include <string.h>

#include "utils/http.h"

#include "utils/http/generics.h"
#include "utils/http/parameter_internal.h"
#include "utils/http/primitives.h"

/**
 * Representation of an HTTP parameter
 */
struct http_parameter {
	http__item base;

	lwc_string *name;		/**< Parameter name */
	lwc_string *value;		/**< Parameter value */
};

/**
 * Destructor for an HTTP parameter
 *
 * \param self  Parameter to destroy
 */
static void http_destroy_parameter(http_parameter *self)
{
	lwc_string_unref(self->name);
	lwc_string_unref(self->value);
	free(self);
}

/**
 * Parse an HTTP parameter
 *
 * \param input      Pointer to current input byte. Updated on exit.
 * \param parameter  Pointer to location to receive on-heap parameter.
 * \return SLATEERROR_OK on success,
 * 	   SLATEERROR_NOMEM on memory exhaustion,
 * 	   SLATEERROR_NOT_FOUND if no parameter could be parsed
 *
 * The returned parameter is owned by the caller.
 */
slateerror http__parse_parameter(const char **input, http_parameter **parameter)
{
	const char *pos = *input;
	lwc_string *name;
	lwc_string *value;
	http_parameter *param;
	slateerror error;

	/* token "=" ( token | quoted-string ) */

	error = http__parse_token(&pos, &name);
	if (error != SLATEERROR_OK)
		return error;

	http__skip_LWS(&pos);

	if (*pos != '=') {
		lwc_string_unref(name);
		return SLATEERROR_NOT_FOUND;
	}

	pos++;

	http__skip_LWS(&pos);

	if (*pos == '"')
		error = http__parse_quoted_string(&pos, &value);
	else
		error = http__parse_token(&pos, &value);

	if (error != SLATEERROR_OK) {
		lwc_string_unref(name);
		return error;
	}

	param = malloc(sizeof(*param));
	if (param == NULL) {
		lwc_string_unref(value);
		lwc_string_unref(name);
		return SLATEERROR_NOMEM;
	}

	HTTP__ITEM_INIT(param, NULL, http_destroy_parameter);
	param->name = name;
	param->value = value;

	*parameter = param;
	*input = pos;

	return SLATEERROR_OK;
}

/* See parameter.h for documentation */
slateerror http_parameter_list_find_item(const http_parameter *list,
		lwc_string *name, lwc_string **value)
{
	bool match;

	while (list != NULL) {
		if (lwc_string_caseless_isequal(name, list->name, 
				&match) == lwc_error_ok && match)
			break;

		list = (http_parameter *) list->base.next;
	}

	if (list == NULL)
		return SLATEERROR_NOT_FOUND;

	*value = lwc_string_ref(list->value);

	return SLATEERROR_OK;
}

/* See parameter.h for documentation */
const http_parameter *http_parameter_list_iterate(const http_parameter *cur,
		lwc_string **name, lwc_string **value)
{
	if (cur == NULL)
		return NULL;

	*name = lwc_string_ref(cur->name);
	*value = lwc_string_ref(cur->value);

	return (http_parameter *) cur->base.next;
}

/* See parameter.h for documentation */
void http_parameter_list_destroy(http_parameter *list)
{
	http__item_list_destroy(list);
}

