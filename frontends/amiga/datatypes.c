/*
 * Copyright 2011 Chris Young <chris@unsatisfactorysoftware.co.uk>
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

#ifdef WITH_AMIGA_DATATYPES
#include "amiga/datatypes.h"

slateerror amiga_datatypes_init(void)
{
	slateerror err = SLATEERROR_OK;

	err = amiga_dt_picture_init();
	if(err != SLATEERROR_OK) return err;

#ifdef WITH_DTANIM
	err = amiga_dt_anim_init();
	if(err != SLATEERROR_OK) return err;
#endif

	err = amiga_dt_sound_init();
	if(err != SLATEERROR_OK) return err;

	return SLATEERROR_OK;
}

#endif
