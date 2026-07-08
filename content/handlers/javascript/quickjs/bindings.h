/*
 * Copyright 2012 Vincent Sanders <vince@netsurf-browser.org>
 * Copyright 2015 Daniel Dilverstone <dsilvers@netsurf-browser.org>
 * Copyright 2016 Michael Drake <tlsa@netsurf-browser.org>
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
 * QuickJS implementation of javascript engine binding functions, prototypes.
 */

#ifndef SLATEJS_BINDINGS_H
#define SLATEJS_BINDINGS_H

slatejs_ret_t slatejs_bind_create_object(slatejs_context *ctx, const char *name, int args);
slatejs_bool_t slatejs_bind_push_node_stacked(slatejs_context *ctx);
slatejs_bool_t slatejs_bind_push_node(slatejs_context *ctx, struct dom_node *node);
void slatejs_bind_inject_not_ctr(slatejs_context *ctx, int idx, const char *name);
void slatejs_bind_register_event_listener_for(slatejs_context *ctx,
				       struct dom_element *ele,
				       dom_string *name,
				       bool capture);
bool slatejs_bind_get_current_value_of_event_handler(slatejs_context *ctx,
					      dom_string *name,
					      dom_event_target *et);
void slatejs_bind_push_event(slatejs_context *ctx, dom_event *evt);
bool slatejs_bind_event_target_push_listeners(slatejs_context *ctx, bool dont_create);

slatejs_bool_t slatejs_bind_check_timeout(void *udata);

typedef enum {
	ELF_CAPTURE = 1 << 0,
	ELF_PASSIVE = 1 << 1,
	ELF_ONCE    = 1 << 2,
	ELF_NONE    = 0
} event_listener_flags;

void slatejs_bind_shuffle_array(slatejs_context *ctx, slatejs_uarridx_t idx);

/* pcall something, and if it errored, also dump the error to the log */
slatejs_int_t slatejs_bind_pcall(slatejs_context *ctx, slatejs_size_t argc, bool reset_timeout);

/* Push a generics function onto the stack */
void slatejs_bind_push_generics(slatejs_context *ctx, const char *generic);

/* Log the current stack frame if possible */
void slatejs_bind_log_stack_frame(slatejs_context *ctx, const char * reason);

#endif
