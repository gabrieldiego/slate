/*
 * Copyright 2011 Daniel Silverstone <dsilvers@digital-scurf.org>
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

#ifndef SLATE_JOTTER_DISPATCH_H
#define SLATE_JOTTER_DISPATCH_H 1

typedef void (*handle_command_fn)(int argc, char **argv);
  
slateerror monkey_register_handler(const char *cmd, handle_command_fn fn);

void monkey_process_command(void);

void monkey_free_handlers(void);

#endif /* SLATE_JOTTER_DISPATCH_H */
