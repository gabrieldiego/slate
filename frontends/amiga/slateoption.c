/*
 * Copyright 2016 Chris Young <chris@unsatisfactorysoftware.co.uk>
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

#include "amiga/os3support.h"

#include <proto/exec.h>
#include <proto/utility.h>

#include "utils/slateoption.h"
#include "amiga/slateoption.h"

static char *current_user_options = NULL;

slateerror ami_slateoption_read(void)
{
	return slateoption_read(current_user_options, NULL);
}

slateerror ami_slateoption_write(void)
{
	return slateoption_write(current_user_options, NULL, NULL);
}

slateerror ami_slateoption_set_location(const char *current_user_dir)
{
	slateerror err = SLATEERROR_OK;

	ami_slateoption_free();

	current_user_options = ASPrintf("%s/Choices", current_user_dir);
	if(current_user_options == NULL)
		err = SLATEERROR_NOMEM;

	return err;
}

void ami_slateoption_free(void)
{
	if(current_user_options != NULL)
		FreeVec(current_user_options);

	current_user_options = NULL;
}

