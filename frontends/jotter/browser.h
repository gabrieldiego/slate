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

#ifndef SLATE_JOTTER_BROWSER_H
#define SLATE_JOTTER_BROWSER_H

struct hlcache_handle;

extern struct gui_window_table *jotter_window_table;
extern struct gui_download_table *jotter_download_table;

struct gui_window {
	struct gui_window *r_next;
	struct gui_window *r_prev;
  
	uint32_t win_num;
	struct browser_window *bw;
  
	int width, height;
	int scrollx, scrolly;
  
	char *host;  /* Ignore this, it's in case RING*() gets debugging for fetchers */
  
};

struct gui_window *jotter_find_window_by_num(uint32_t win_num);
struct gui_window *jotter_find_window_by_content(struct hlcache_handle *content);
void jotter_window_process_reformats(void);

void jotter_window_handle_command(int argc, char **argv);
void jotter_kill_browser_windows(void);

slateerror jotter_warn_user(const char *warning, const char *detail);

#endif /* SLATE_JOTTER_BROWSER_H */
