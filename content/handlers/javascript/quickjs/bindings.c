/*
 * Copyright 2012 Vincent Sanders <vince@netsurf-browser.org>
 * Copyright 2015 Daniel Dilverstone <dsilvers@netsurf-browser.org>
 * Copyright 2016 Michael Drake <tlsa@netsurf-browser.org>
 * Copyright 2016 John-Mark Bell <jmb@netsurf-browser.org>
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
 * QuickJS implementation of javascript engine binding functions.
 */

#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <nsutils/time.h>

#include "slate/inttypes.h"
#include "utils/utils.h"
#include "utils/slateoption.h"
#include "utils/log.h"
#include "utils/corestrings.h"
#include "content/content.h"

#include "javascript/js.h"
#include "javascript/content.h"

#include "quickjs/binding.h"
#include "quickjs/generics.js.inc"
#include "quickjs/polyfill.js.inc"

#include "api.h"
#include "bindings.h"

#include <dom/dom.h>

#define EVENT_MAGIC MAGIC(EVENT_MAP)
#define HANDLER_LISTENER_MAGIC MAGIC(HANDLER_LISTENER_MAP)
#define HANDLER_MAGIC MAGIC(HANDLER_MAP)
#define EVENT_LISTENER_JS_MAGIC MAGIC(EVENT_LISTENER_JS_MAP)
#define GENERICS_MAGIC MAGIC(GENERICS_TABLE)
#define THREAD_MAP MAGIC(THREAD_MAP)

/**
 * slatejs javascript heap
 */
struct jsheap {
	slatejs_context *ctx; /**< quickjs base context */
	slatejs_uarridx_t next_thread; /**< monotonic thread counter */
	bool pending_destroy; /**< Whether this heap is pending destruction */
	unsigned int live_threads; /**< number of live threads */
	uint64_t exec_start_time;
};

/**
 * slatejs javascript thread
 */
struct jsthread {
	bool pending_destroy; /**< Whether this thread is pending destruction */
	unsigned int in_use; /**< The number of times this thread is in use */
	jsheap *heap; /**< The heap this thread belongs to */
	slatejs_context *ctx; /**< The quickjs thread context */
	slatejs_uarridx_t thread_idx; /**< The thread number */
};

static slatejs_ret_t slatejs_populate_object(slatejs_context *ctx, void *udata)
{
	/* ... obj args protoname nargs */
	int nargs = slatejs_get_int(ctx, -1);
	slatejs_pop(ctx);
	/* ... obj args protoname */
	slatejs_get_global_string(ctx, PROTO_MAGIC);
	/* .. obj args protoname prototab */
	slatejs_dup(ctx, -2);
	/* ... obj args protoname prototab protoname */
	slatejs_get_prop(ctx, -2);
	/* ... obj args protoname prototab {proto/undefined} */
	if (slatejs_is_undefined(ctx, -1)) {
		NSLOG(quickjs, WARNING,
		      "Unable to find slatejs prototype for `%s` - falling back to HTMLUnknownElement",
		      slatejs_get_string(ctx, -3) + 2 /* Skip the two unprintables */);
		slatejs_pop(ctx);
		slatejs_push_string(ctx, PROTO_NAME(HTMLUNKNOWNELEMENT));
		slatejs_get_prop(ctx, -2);
	}
	/* ... obj args protoname prototab proto */
	slatejs_remove(ctx, -3);
	/* ... obj args prototab proto */
	slatejs_dup(ctx, -1);
	/* ... obj args prototab proto proto */
	slatejs_set_prototype(ctx, -(nargs+4));
	/* ... obj[proto] args prototab proto */
	slatejs_get_prop_string(ctx, -1, INIT_MAGIC);
	/* ... obj[proto] args prototab proto initfn */
	slatejs_insert(ctx, -(nargs+4));
	/* ... initfn obj[proto] args prototab proto */
	slatejs_pop_2(ctx);
	/* ... initfn obj[proto] args */
	NSLOG(quickjs, DEEPDEBUG, "Call the init function");
	slatejs_call(ctx, nargs + 1);
	return 1; /* The object */
}

slatejs_ret_t slatejs_bind_create_object(slatejs_context *ctx, const char *name, int args)
{
	slatejs_ret_t ret;
	NSLOG(quickjs, DEEPDEBUG, "name=%s nargs=%d", name + 2, args);
	/* ... args */
	slatejs_push_object(ctx);
	/* ... args obj */
	slatejs_push_object(ctx);
	/* ... args obj handlers */
	slatejs_put_prop_string(ctx, -2, HANDLER_LISTENER_MAGIC);
	/* ... args obj */
	slatejs_push_object(ctx);
	/* ... args obj handlers */
	slatejs_put_prop_string(ctx, -2, HANDLER_MAGIC);
	/* ... args obj */
	slatejs_insert(ctx, -(args+1));
	/* ... obj args */
	slatejs_push_string(ctx, name);
	/* ... obj args name */
	slatejs_push_int(ctx, args);
	/* ... obj args name nargs */
	if ((ret = slatejs_safe_call(ctx, slatejs_populate_object, NULL, args + 3, 1))
	    != SLATEJS_EXEC_SUCCESS)
		return ret;
	NSLOG(quickjs, DEEPDEBUG, "created");
	return SLATEJS_EXEC_SUCCESS;
}



slatejs_bool_t
slatejs_bind_push_node_stacked(slatejs_context *ctx)
{
	int top_at_fail = slatejs_get_top(ctx) - 2;
	/* ... nodeptr klass */
	slatejs_get_global_string(ctx, NODE_MAGIC);
	/* ... nodeptr klass nodes */
	slatejs_dup(ctx, -3);
	/* ... nodeptr klass nodes nodeptr */
	slatejs_get_prop(ctx, -2);
	/* ... nodeptr klass nodes node/undefined */
	if (slatejs_is_undefined(ctx, -1)) {
		/* ... nodeptr klass nodes undefined */
		slatejs_pop(ctx);
		/* ... nodeptr klass nodes */
		slatejs_push_object(ctx);
		/* ... nodeptr klass nodes obj */
		slatejs_push_object(ctx);
		/* ... nodeptr klass nodes obj handlers */
		slatejs_put_prop_string(ctx, -2, HANDLER_LISTENER_MAGIC);
		/* ... nodeptr klass nodes obj */
		slatejs_push_object(ctx);
		/* ... nodeptr klass nodes obj handlers */
		slatejs_put_prop_string(ctx, -2, HANDLER_MAGIC);
		/* ... nodeptr klass nodes obj */
		slatejs_dup(ctx, -4);
		/* ... nodeptr klass nodes obj nodeptr */
		slatejs_dup(ctx, -4);
		/* ... nodeptr klass nodes obj nodeptr klass */
		slatejs_push_int(ctx, 1);
		/* ... nodeptr klass nodes obj nodeptr klass 1 */
		if (slatejs_safe_call(ctx, slatejs_populate_object, NULL, 4, 1)
		    != SLATEJS_EXEC_SUCCESS) {
			slatejs_set_top(ctx, top_at_fail);
			NSLOG(quickjs, ERROR, "Failed to populate object prototype");
			return false;
		}
		/* ... nodeptr klass nodes node */
		slatejs_dup(ctx, -4);
		/* ... nodeptr klass nodes node nodeptr */
		slatejs_dup(ctx, -2);
		/* ... nodeptr klass nodes node nodeptr node */
		slatejs_put_prop(ctx, -4);
		/* ... nodeptr klass nodes node */
	}
	/* ... nodeptr klass nodes node */
	slatejs_insert(ctx, -4);
	/* ... node nodeptr klass nodes */
	slatejs_pop_3(ctx);
	/* ... node */
	if (NSLOG_COMPILED_MIN_LEVEL <= NSLOG_LEVEL_DEEPDEBUG) {
		slatejs_dup(ctx, -1);
		const char * what = slatejs_safe_to_string(ctx, -1);
		NSLOG(quickjs, DEEPDEBUG, "Created: %s", what);
		slatejs_pop(ctx);
	}
	return true;
}

#define SET_HTML_CLASS(CLASS) \
		*html_class = PROTO_NAME(HTML##CLASS##ELEMENT); \
		*html_class_len = \
				SLEN(PROTO_NAME(HTML)) + \
				SLEN(#CLASS) + \
				SLEN("ELEMENT");

static void slatejs_html_element_class_from_tag_type(dom_html_element_type type,
		const char **html_class, size_t *html_class_len)
{
	switch(type) {
	case DOM_HTML_ELEMENT_TYPE_HTML:
		SET_HTML_CLASS(HTML)
		break;
	case DOM_HTML_ELEMENT_TYPE_HEAD:
		SET_HTML_CLASS(HEAD)
		break;
	case DOM_HTML_ELEMENT_TYPE_META:
		SET_HTML_CLASS(META)
		break;
	case DOM_HTML_ELEMENT_TYPE_BASE:
		SET_HTML_CLASS(BASE)
		break;
	case DOM_HTML_ELEMENT_TYPE_TITLE:
		SET_HTML_CLASS(TITLE)
		break;
	case DOM_HTML_ELEMENT_TYPE_BODY:
		SET_HTML_CLASS(BODY)
		break;
	case DOM_HTML_ELEMENT_TYPE_DIV:
		SET_HTML_CLASS(DIV)
		break;
	case DOM_HTML_ELEMENT_TYPE_SPAN:
		SET_HTML_CLASS(SPAN)
		break;
	case DOM_HTML_ELEMENT_TYPE_FORM:
		SET_HTML_CLASS(FORM)
		break;
	case DOM_HTML_ELEMENT_TYPE_LINK:
		SET_HTML_CLASS(LINK)
		break;
	case DOM_HTML_ELEMENT_TYPE_BUTTON:
		SET_HTML_CLASS(BUTTON)
		break;
	case DOM_HTML_ELEMENT_TYPE_INPUT:
		SET_HTML_CLASS(INPUT)
		break;
	case DOM_HTML_ELEMENT_TYPE_TEXTAREA:
		SET_HTML_CLASS(TEXTAREA)
		break;
	case DOM_HTML_ELEMENT_TYPE_DATALIST:
		SET_HTML_CLASS(DATALIST)
		break;
	case DOM_HTML_ELEMENT_TYPE_OPTGROUP:
		SET_HTML_CLASS(OPTGROUP)
		break;
	case DOM_HTML_ELEMENT_TYPE_OPTION:
		SET_HTML_CLASS(OPTION)
		break;
	case DOM_HTML_ELEMENT_TYPE_SELECT:
		SET_HTML_CLASS(SELECT)
		break;
	case DOM_HTML_ELEMENT_TYPE_HR:
		SET_HTML_CLASS(HR)
		break;
	case DOM_HTML_ELEMENT_TYPE_DL:
		SET_HTML_CLASS(DLIST)
		break;
	case DOM_HTML_ELEMENT_TYPE_DIR:
		SET_HTML_CLASS(DIRECTORY)
		break;
	case DOM_HTML_ELEMENT_TYPE_MENU:
		SET_HTML_CLASS(MENU)
		break;
	case DOM_HTML_ELEMENT_TYPE_MENUITEM:
		SET_HTML_CLASS(MENUITEM)
		break;
	case DOM_HTML_ELEMENT_TYPE_FIELDSET:
		SET_HTML_CLASS(FIELDSET)
		break;
	case DOM_HTML_ELEMENT_TYPE_LEGEND:
		SET_HTML_CLASS(LEGEND)
		break;
	case DOM_HTML_ELEMENT_TYPE_OUTPUT:
		SET_HTML_CLASS(OUTPUT)
		break;
	case DOM_HTML_ELEMENT_TYPE_METER:
		SET_HTML_CLASS(METER)
		break;
	case DOM_HTML_ELEMENT_TYPE_PROGRESS:
		SET_HTML_CLASS(PROGRESS)
		break;
	case DOM_HTML_ELEMENT_TYPE_DETAILS:
		SET_HTML_CLASS(DETAILS)
		break;
	case DOM_HTML_ELEMENT_TYPE_DIALOG:
		SET_HTML_CLASS(DIALOG)
		break;
	case DOM_HTML_ELEMENT_TYPE_P:
		SET_HTML_CLASS(PARAGRAPH)
		break;
	case DOM_HTML_ELEMENT_TYPE_H1:
	case DOM_HTML_ELEMENT_TYPE_H2:
	case DOM_HTML_ELEMENT_TYPE_H3:
	case DOM_HTML_ELEMENT_TYPE_H4:
	case DOM_HTML_ELEMENT_TYPE_H5:
	case DOM_HTML_ELEMENT_TYPE_H6:
		SET_HTML_CLASS(HEADING)
		break;
	case DOM_HTML_ELEMENT_TYPE_BLOCKQUOTE:
	case DOM_HTML_ELEMENT_TYPE_Q:
		SET_HTML_CLASS(QUOTE)
		break;
	case DOM_HTML_ELEMENT_TYPE_PRE:
		SET_HTML_CLASS(PRE)
		break;
	case DOM_HTML_ELEMENT_TYPE_BR:
		SET_HTML_CLASS(BR)
		break;
	case DOM_HTML_ELEMENT_TYPE_LABEL:
		SET_HTML_CLASS(LABEL)
		break;
	case DOM_HTML_ELEMENT_TYPE_UL:
		SET_HTML_CLASS(ULIST)
		break;
	case DOM_HTML_ELEMENT_TYPE_OL:
		SET_HTML_CLASS(OLIST)
		break;
	case DOM_HTML_ELEMENT_TYPE_LI:
		SET_HTML_CLASS(LI)
		break;
	case DOM_HTML_ELEMENT_TYPE_FONT:
		SET_HTML_CLASS(FONT)
		break;
	case DOM_HTML_ELEMENT_TYPE_MARQUEE:
		SET_HTML_CLASS(MARQUEE)
		break;
	case DOM_HTML_ELEMENT_TYPE_DEL:
	case DOM_HTML_ELEMENT_TYPE_INS:
		SET_HTML_CLASS(MOD)
		break;
	case DOM_HTML_ELEMENT_TYPE_DATA:
		SET_HTML_CLASS(DATA)
		break;
	case DOM_HTML_ELEMENT_TYPE_TIME:
		SET_HTML_CLASS(TIME)
		break;
	case DOM_HTML_ELEMENT_TYPE_A:
		SET_HTML_CLASS(ANCHOR)
		break;
	case DOM_HTML_ELEMENT_TYPE_BASEFONT:
		SET_HTML_CLASS(BASEFONT)
		break;
	case DOM_HTML_ELEMENT_TYPE_IMG:
		SET_HTML_CLASS(IMAGE)
		break;
	case DOM_HTML_ELEMENT_TYPE_AUDIO:
		SET_HTML_CLASS(AUDIO)
		break;
	case DOM_HTML_ELEMENT_TYPE_VIDEO:
		SET_HTML_CLASS(VIDEO)
		break;
	case DOM_HTML_ELEMENT_TYPE_SOURCE:
		SET_HTML_CLASS(SOURCE)
		break;
	case DOM_HTML_ELEMENT_TYPE_TRACK:
		SET_HTML_CLASS(TRACK)
		break;
	case DOM_HTML_ELEMENT_TYPE_PICTURE:
		SET_HTML_CLASS(PICTURE)
		break;
	case DOM_HTML_ELEMENT_TYPE_EMBED:
		SET_HTML_CLASS(EMBED)
		break;
	case DOM_HTML_ELEMENT_TYPE_OBJECT:
		SET_HTML_CLASS(OBJECT)
		break;
	case DOM_HTML_ELEMENT_TYPE_PARAM:
		SET_HTML_CLASS(PARAM)
		break;
	case DOM_HTML_ELEMENT_TYPE_APPLET:
		SET_HTML_CLASS(APPLET)
		break;
	case DOM_HTML_ELEMENT_TYPE_MAP:
		SET_HTML_CLASS(MAP)
		break;
	case DOM_HTML_ELEMENT_TYPE_AREA:
		SET_HTML_CLASS(AREA)
		break;
	case DOM_HTML_ELEMENT_TYPE_SCRIPT:
		SET_HTML_CLASS(SCRIPT)
		break;
	case DOM_HTML_ELEMENT_TYPE_CAPTION:
		SET_HTML_CLASS(TABLECAPTION)
		break;
	case DOM_HTML_ELEMENT_TYPE_TD:
		SET_HTML_CLASS(TABLEDATACELL)
		break;
	case DOM_HTML_ELEMENT_TYPE_TH:
		SET_HTML_CLASS(TABLEHEADERCELL)
		break;
	case DOM_HTML_ELEMENT_TYPE_COL:
	case DOM_HTML_ELEMENT_TYPE_COLGROUP:
		SET_HTML_CLASS(TABLECOL)
		break;
	case DOM_HTML_ELEMENT_TYPE_THEAD:
	case DOM_HTML_ELEMENT_TYPE_TBODY:
	case DOM_HTML_ELEMENT_TYPE_TFOOT:
		SET_HTML_CLASS(TABLESECTION)
		break;
	case DOM_HTML_ELEMENT_TYPE_TABLE:
		SET_HTML_CLASS(TABLE)
		break;
	case DOM_HTML_ELEMENT_TYPE_TR:
		SET_HTML_CLASS(TABLEROW)
		break;
	case DOM_HTML_ELEMENT_TYPE_STYLE:
		SET_HTML_CLASS(STYLE)
		break;
	case DOM_HTML_ELEMENT_TYPE_FRAMESET:
		SET_HTML_CLASS(FRAMESET)
		break;
	case DOM_HTML_ELEMENT_TYPE_FRAME:
		SET_HTML_CLASS(FRAME)
		break;
	case DOM_HTML_ELEMENT_TYPE_IFRAME:
		SET_HTML_CLASS(IFRAME)
		break;
	case DOM_HTML_ELEMENT_TYPE_ISINDEX:
		SET_HTML_CLASS(ISINDEX)
		break;
	case DOM_HTML_ELEMENT_TYPE_KEYGEN:
		SET_HTML_CLASS(KEYGEN)
		break;
	case DOM_HTML_ELEMENT_TYPE_CANVAS:
		SET_HTML_CLASS(CANVAS)
		break;
	case DOM_HTML_ELEMENT_TYPE_TEMPLATE:
		SET_HTML_CLASS(TEMPLATE)
		break;
	case DOM_HTML_ELEMENT_TYPE__COUNT:
		assert(type != DOM_HTML_ELEMENT_TYPE__COUNT);
		fallthrough;
	case DOM_HTML_ELEMENT_TYPE__UNKNOWN:
		SET_HTML_CLASS(UNKNOWN)
		break;
	default:
		/* Known HTML element without a specialisation */
		*html_class = PROTO_NAME(HTMLELEMENT);
		*html_class_len =
				SLEN(PROTO_NAME(HTML)) +
				SLEN("ELEMENT");
		break;
	}
	return;
}

#undef SET_HTML_CLASS

static void
slatejs_bind_push_node_klass(slatejs_context *ctx, struct dom_node *node)
{
	dom_node_type nodetype;
	dom_exception err;

	err = dom_node_get_node_type(node, &nodetype);
	if (err != DOM_NO_ERR) {
		/* Oh bum, just node then */
		slatejs_push_string(ctx, PROTO_NAME(NODE));
		return;
	}

	switch(nodetype) {
	case DOM_ELEMENT_NODE: {
		dom_string *namespace;
		dom_html_element_type type;
		const char *html_class;
		size_t html_class_len;
		err = dom_node_get_namespace(node, &namespace);
		if (err != DOM_NO_ERR) {
			/* Feck it, element */
			NSLOG(quickjs, ERROR,
			      "dom_node_get_namespace() failed");
			slatejs_push_string(ctx, PROTO_NAME(ELEMENT));
			break;
		}
		if (namespace == NULL) {
			/* No namespace, -> element */
			NSLOG(quickjs, DEBUG, "no namespace");
			slatejs_push_string(ctx, PROTO_NAME(ELEMENT));
			break;
		}

		if (dom_string_isequal(namespace, corestring_dom_html_namespace) == false) {
			/* definitely not an HTML element of some kind */
			slatejs_push_string(ctx, PROTO_NAME(ELEMENT));
			dom_string_unref(namespace);
			break;
		}
		dom_string_unref(namespace);

		err = dom_html_element_get_tag_type(node, &type);
		if (err != DOM_NO_ERR) {
			type = DOM_HTML_ELEMENT_TYPE__UNKNOWN;
		}

		slatejs_html_element_class_from_tag_type(type,
				&html_class, &html_class_len);

		slatejs_push_lstring(ctx, html_class, html_class_len);
		break;
	}
	case DOM_TEXT_NODE:
		slatejs_push_string(ctx, PROTO_NAME(TEXT));
		break;
	case DOM_COMMENT_NODE:
		slatejs_push_string(ctx, PROTO_NAME(COMMENT));
		break;
	case DOM_DOCUMENT_NODE:
		slatejs_push_string(ctx, PROTO_NAME(DOCUMENT));
		break;
	case DOM_ATTRIBUTE_NODE:
	case DOM_PROCESSING_INSTRUCTION_NODE:
	case DOM_DOCUMENT_TYPE_NODE:
	case DOM_DOCUMENT_FRAGMENT_NODE:
	case DOM_NOTATION_NODE:
	case DOM_ENTITY_REFERENCE_NODE:
	case DOM_ENTITY_NODE:
	case DOM_CDATA_SECTION_NODE:
	default:
		/* Oh bum, just node then */
		slatejs_push_string(ctx, PROTO_NAME(NODE));
	}
}

slatejs_bool_t
slatejs_bind_push_node(slatejs_context *ctx, struct dom_node *node)
{
	NSLOG(quickjs, DEEPDEBUG, "Pushing node %p", node);
	/* First check if we can find the node */
	/* ... */
	slatejs_get_global_string(ctx, NODE_MAGIC);
	/* ... nodes */
	slatejs_push_pointer(ctx, node);
	/* ... nodes nodeptr */
	slatejs_get_prop(ctx, -2);
	/* ... nodes node/undefined */
	if (!slatejs_is_undefined(ctx, -1)) {
		/* ... nodes node */
		slatejs_insert(ctx, -2);
		/* ... node nodes */
		slatejs_pop(ctx);
		/* ... node */
		if (NSLOG_COMPILED_MIN_LEVEL <= NSLOG_LEVEL_DEEPDEBUG) {
			slatejs_dup(ctx, -1);
			const char * what = slatejs_safe_to_string(ctx, -1);
			NSLOG(quickjs, DEEPDEBUG, "Found it memoised: %s", what);
			slatejs_pop(ctx);
		}
		return true;
	}
	/* ... nodes undefined */
	slatejs_pop_2(ctx);
	/* ... */
	/* We couldn't, so now we determine the node type and then
	 * we ask for it to be created
	 */
	slatejs_push_pointer(ctx, node);
	/* ... nodeptr */
	slatejs_bind_push_node_klass(ctx, node);
	/* ... nodeptr klass */
	return slatejs_bind_push_node_stacked(ctx);
}

static slatejs_ret_t
slatejs_bind_bad_constructor(slatejs_context *ctx)
{
	return slatejs_error(ctx, SLATEJS_ERR_ERROR, "Bad constructor");
}

void
slatejs_bind_inject_not_ctr(slatejs_context *ctx, int idx, const char *name)
{
	/* ... p[idx] ... proto */
	slatejs_push_c_function(ctx, slatejs_bind_bad_constructor, 0);
	/* ... p[idx] ... proto cons */
	slatejs_insert(ctx, -2);
	/* ... p[idx] ... cons proto */
	slatejs_put_prop_string(ctx, -2, "prototype");
	/* ... p[idx] ... cons[proto] */
	slatejs_put_prop_string(ctx, idx, name);
	/* ... p ... */
	return;
}

/* QuickJS heap utility functions */

/* We need to override the defaults because not all platforms are fully ANSI
 * compatible.  E.g. RISC OS gets upset if we malloc or realloc a zero byte
 * block, as do debugging tools such as Electric Fence by Bruce Perens.
 */

static void *slatejs_heap_alloc(void *udata, slatejs_size_t size)
{
	if (size == 0)
		return NULL;

	return malloc(size);
}

static void *slatejs_heap_realloc(void *udata, void *ptr, slatejs_size_t size)
{
	if (ptr == NULL && size == 0)
		return NULL;

	if (size == 0) {
		free(ptr);
		return NULL;
	}

	return realloc(ptr, size);
}


static void slatejs_heap_free(void *udata, void *ptr)
{
	if (ptr != NULL)
		free(ptr);
}

/* exported interface documented in js.h */
void js_initialise(void)
{
	/** TODO: Forces JS on for our testing, needs changing before a release
	 * lest we incur the wrath of others.
	 */
	/* Disabled force-on for forthcoming release */
	/* slateoption_set_bool(enable_javascript, true);
	 */
	javascript_init();
}


/* exported interface documented in js.h */
void js_finalise(void)
{
	/* NADA for now */
}


/* exported interface documented in js.h */
slateerror
js_newheap(int timeout, jsheap **heap)
{
	slatejs_context *ctx;
	jsheap *ret = calloc(1, sizeof(*ret));
	*heap = NULL;
	NSLOG(quickjs, DEBUG, "Creating new quickjs javascript heap");
	if (ret == NULL) return SLATEERROR_NOMEM;
	ctx = ret->ctx = slatejs_create_heap(
		slatejs_heap_alloc,
		slatejs_heap_realloc,
		slatejs_heap_free,
		ret,
		NULL);
	if (ret->ctx == NULL) { free(ret); return SLATEERROR_NOMEM; }
	/* Create the prototype stuffs */
	slatejs_push_global_object(ctx);
	slatejs_push_boolean(ctx, true);
	slatejs_put_prop_string(ctx, -2, "protos");
	slatejs_put_global_string(ctx, PROTO_MAGIC);
	/* Create prototypes here */
	slatejs_bind_create_prototypes(ctx);
	/* Now create the thread map */
	slatejs_push_object(ctx);
	slatejs_put_global_string(ctx, THREAD_MAP);

	*heap = ret;
	return SLATEERROR_OK;
}


static void slatejs_destroyheap(jsheap *heap)
{
	assert(heap->pending_destroy == true);
	assert(heap->live_threads == 0);
	NSLOG(quickjs, DEBUG, "Destroying quickjs javascript context");
	slatejs_destroy_heap(heap->ctx);
	free(heap);
}

/* exported interface documented in js.h */
void js_destroyheap(jsheap *heap)
{
	heap->pending_destroy = true;
	if (heap->live_threads == 0) {
		slatejs_destroyheap(heap);
	}
}

/* Just for here, the CTX is in ret, not thread */
#define CTX (ret->ctx)

/* exported interface documented in js.h */
slateerror js_newthread(jsheap *heap, void *win_priv, void *doc_priv, jsthread **thread)
{
	jsthread *ret;
	assert(heap != NULL);
	assert(heap->pending_destroy == false);

	ret = calloc(1, sizeof (*ret));
	if (ret == NULL) {
		NSLOG(quickjs, ERROR, "Unable to allocate new JS thread structure");
		return SLATEERROR_NOMEM;
	}

	NSLOG(quickjs, DEBUG,
	      "New javascript/quickjs thread, win_priv=%p, doc_priv=%p",
	      win_priv, doc_priv);

	/* create new thread */
	slatejs_get_global_string(heap->ctx, THREAD_MAP); /* ... threads */
	slatejs_push_thread(heap->ctx); /* ... threads thread */
	ret->heap = heap;
	ret->ctx = slatejs_require_context(heap->ctx, -1);
	ret->thread_idx = heap->next_thread++;
	slatejs_put_prop_index(heap->ctx, -2, ret->thread_idx);
	heap->live_threads++;
	slatejs_pop(heap->ctx); /* ... */
	slatejs_push_int(CTX, 0);
	slatejs_push_int(CTX, 1);
	slatejs_push_int(CTX, 2);
	/* Manufacture a Window object */
	/* win_priv is a browser_window, doc_priv is an html content struct */
	slatejs_push_pointer(CTX, win_priv);
	slatejs_push_pointer(CTX, doc_priv);
	slatejs_bind_create_object(CTX, PROTO_NAME(WINDOW), 2);
	slatejs_push_global_object(CTX);
	slatejs_put_prop_string(CTX, -2, PROTO_MAGIC);
	slatejs_set_global_object(CTX);

	/* Now we need to prepare our node mapping table */
	slatejs_push_object(CTX);
	slatejs_push_pointer(CTX, NULL);
	slatejs_push_null(CTX);
	slatejs_put_prop(CTX, -3);
	slatejs_put_global_string(CTX, NODE_MAGIC);

	/* And now the event mapping table */
	slatejs_push_object(CTX);
	slatejs_put_global_string(CTX, EVENT_MAGIC);

	/* Now load the polyfills */
	/* ... */
	slatejs_push_string(CTX, "polyfill.js");
	/* ..., polyfill.js */
	if (slatejs_pcompile_lstring_filename(CTX, SLATEJS_COMPILE_EVAL,
					  (const char *)polyfill_js, polyfill_js_len) != 0) {
		NSLOG(quickjs, CRITICAL, "%s", slatejs_safe_to_string(CTX, -1));
		NSLOG(quickjs, CRITICAL, "Unable to compile polyfill.js, thread aborted");
		js_destroythread(ret);
		return SLATEERROR_INIT_FAILED;
	}
	/* ..., (generics.js) */
	if (slatejs_bind_pcall(CTX, 0, true) != 0) {
		NSLOG(quickjs, CRITICAL, "Unable to run polyfill.js, thread aborted");
		js_destroythread(ret);
		return SLATEERROR_INIT_FAILED;
	}
	/* ..., result */
	slatejs_pop(CTX);
	/* ... */

	/* Now load the NetSurf table in */
	/* ... */
	slatejs_push_string(CTX, "generics.js");
	/* ..., generics.js */
	if (slatejs_pcompile_lstring_filename(CTX, SLATEJS_COMPILE_EVAL,
					  (const char *)generics_js, generics_js_len) != 0) {
		NSLOG(quickjs, CRITICAL, "%s", slatejs_safe_to_string(CTX, -1));
		NSLOG(quickjs, CRITICAL, "Unable to compile generics.js, thread aborted");
		js_destroythread(ret);
		return SLATEERROR_INIT_FAILED;
	}
	/* ..., (generics.js) */
	if (slatejs_bind_pcall(CTX, 0, true) != 0) {
		NSLOG(quickjs, CRITICAL, "Unable to run generics.js, thread aborted");
		js_destroythread(ret);
		return SLATEERROR_INIT_FAILED;
	}
	/* ..., result */
	slatejs_pop(CTX);
	/* ... */
	slatejs_push_global_object(CTX);
	/* ..., Win */
	slatejs_get_prop_string(CTX, -1, "NetSurf");
	/* ..., Win, NetSurf */
	slatejs_put_global_string(CTX, GENERICS_MAGIC);
	/* ..., Win */
	slatejs_del_prop_string(CTX, -1, "NetSurf");
	slatejs_pop(CTX);
	/* ... */

	slatejs_bind_log_stack_frame(CTX, "New thread created");
	NSLOG(quickjs, DEBUG, "New thread is %p in heap %p", thread, heap);
	*thread = ret;

	return SLATEERROR_OK;
}

/* Now switch to the long term CTX behaviour */
#undef CTX
#define CTX (thread->ctx)

/* exported interface documented in js.h */
slateerror js_closethread(jsthread *thread)
{
	/* We can always close down a thread, it might just confuse
	 * the code running, though we don't mind since we're in the
	 * process of destruction at this point
	 */
	slatejs_int_t top = slatejs_get_top(CTX);

	/* Closing down the extant thread */
	NSLOG(quickjs, DEBUG, "Closing down extant thread %p in heap %p", thread, thread->heap);
	slatejs_get_global_string(CTX, MAGIC(closedownThread));
	slatejs_bind_pcall(CTX, 0, true);

	/* Restore whatever stack we had */
	slatejs_set_top(CTX, top);

	return SLATEERROR_OK;
}

/**
 * Destroy a QuickJS thread
 */
static void slatejs_destroythread(jsthread *thread)
{
	jsheap *heap = thread->heap;

	assert(thread->in_use == 0);
	assert(thread->pending_destroy == true);

	/* Closing down the extant thread */
	NSLOG(quickjs, DEBUG, "Closing down extant thread %p in heap %p", thread, heap);
	slatejs_get_global_string(CTX, MAGIC(closedownThread));
	slatejs_bind_pcall(CTX, 0, true);

	/* Now delete the thread from the heap */
	slatejs_get_global_string(heap->ctx, THREAD_MAP); /* ... threads */
	slatejs_del_prop_index(heap->ctx, -1, thread->thread_idx);
	slatejs_pop(heap->ctx); /* ... */

	/* We can now free the thread object */
	free(thread);

	/* Finally give the heap a chance to clean up */
	slatejs_gc(heap->ctx, 0);
	slatejs_gc(heap->ctx, SLATEJS_GC_COMPACT);
	heap->live_threads--;

	/* And if the heap should now go, blow it away */
	if (heap->pending_destroy == true && heap->live_threads == 0) {
		slatejs_destroyheap(heap);
	}
}

/* exported interface documented in js.h */
void js_destroythread(jsthread *thread)
{
	thread->pending_destroy = true;
	if (thread->in_use == 0) {
		slatejs_destroythread(thread);
	}
}

static void slatejs_enter_thread(jsthread *thread)
{
	assert(thread != NULL);
	thread->in_use++;
}

static void slatejs_leave_thread(jsthread *thread)
{
	assert(thread != NULL);
	assert(thread->in_use > 0);

	thread->in_use--;
	if (thread->in_use == 0 && thread->pending_destroy == true) {
		slatejs_destroythread(thread);
	}
}

slatejs_bool_t slatejs_bind_check_timeout(void *udata)
{
#define JS_EXEC_TIMEOUT_MS 10000 /* 10 seconds */
	jsheap *heap = (jsheap *) udata;
	uint64_t now;

	(void) nsu_getmonotonic_ms(&now);

	/* This function may be called during duk heap construction,
	 * so only test for execution timeout if we've recorded a
	 * start time.
	 */
	return heap->exec_start_time != 0 &&
			now > (heap->exec_start_time + JS_EXEC_TIMEOUT_MS);
}

static bool slatejs_jsbuf_append(char **buf, size_t *len, size_t *cap, const char *data, size_t data_len)
{
	char *nbuf;
	size_t ncap;

	if (data_len == 0) {
		return true;
	}

	if (*len + data_len + 1 <= *cap) {
		memcpy(*buf + *len, data, data_len);
		*len += data_len;
		(*buf)[*len] = '\0';
		return true;
	}

	ncap = *cap;
	while (*len + data_len + 1 > ncap) {
		ncap *= 2;
	}

	nbuf = realloc(*buf, ncap);
	if (nbuf == NULL) {
		return false;
	}

	*buf = nbuf;
	*cap = ncap;
	memcpy(*buf + *len, data, data_len);
	*len += data_len;
	(*buf)[*len] = '\0';
	return true;
}

static bool slatejs_jsbuf_append_ch(char **buf, size_t *len, size_t *cap, char ch)
{
	return slatejs_jsbuf_append(buf, len, cap, &ch, 1);
}

static bool slatejs_jsbuf_append_js_string_ch(char **buf, size_t *len, size_t *cap, char ch)
{
	switch (ch) {
	case '"':
		return slatejs_jsbuf_append(buf, len, cap, "\\\"", 2);
	case '\\':
		return slatejs_jsbuf_append(buf, len, cap, "\\\\", 2);
	case '\n':
		return slatejs_jsbuf_append(buf, len, cap, "\\n", 2);
	case '\r':
		return slatejs_jsbuf_append(buf, len, cap, "\\r", 2);
	case '\t':
		return slatejs_jsbuf_append(buf, len, cap, "\\t", 2);
	default:
		return slatejs_jsbuf_append_ch(buf, len, cap, ch);
	}
}

static bool slatejs_transform_template_literal(const char *src, size_t src_len, size_t *idx,
		char **buf, size_t *len, size_t *cap)
{
	size_t i = *idx + 1; /* skip opening backtick */
	unsigned int expr_depth;
	char quote;

	if (!slatejs_jsbuf_append(buf, len, cap, "(\"", 2)) {
		return false;
	}

	while (i < src_len) {
		char ch = src[i];

		if (ch == '`') {
			if (!slatejs_jsbuf_append(buf, len, cap, "\")", 2)) {
				return false;
			}
			*idx = i + 1;
			return true;
		}

		if (ch == '\\') {
			if (i + 1 >= src_len) {
				return false;
			}
			i++;
			if (!slatejs_jsbuf_append_js_string_ch(buf, len, cap, src[i])) {
				return false;
			}
			i++;
			continue;
		}

		if (ch == '$' && i + 1 < src_len && src[i + 1] == '{') {
			if (!slatejs_jsbuf_append(buf, len, cap, "\"+(", 3)) {
				return false;
			}
			i += 2;
			expr_depth = 1;

			while (i < src_len && expr_depth > 0) {
				ch = src[i];

				if (ch == '\'' || ch == '"') {
					quote = ch;
					if (!slatejs_jsbuf_append_ch(buf, len, cap, ch)) {
						return false;
					}
					i++;
					while (i < src_len) {
						ch = src[i];
						if (!slatejs_jsbuf_append_ch(buf, len, cap, ch)) {
							return false;
						}
						i++;
						if (ch == '\\' && i < src_len) {
							if (!slatejs_jsbuf_append_ch(buf, len, cap, src[i])) {
								return false;
							}
							i++;
						} else if (ch == quote) {
							break;
						}
					}
					continue;
				}

				if (ch == '{') {
					expr_depth++;
				} else if (ch == '}') {
					expr_depth--;
					if (expr_depth == 0) {
						i++;
						break;
					}
				}

				if (!slatejs_jsbuf_append_ch(buf, len, cap, ch)) {
					return false;
				}
				i++;
			}

			if (expr_depth != 0 || !slatejs_jsbuf_append(buf, len, cap, ")+\"", 3)) {
				return false;
			}
			continue;
		}

		if (!slatejs_jsbuf_append_js_string_ch(buf, len, cap, ch)) {
			return false;
		}
		i++;
	}

	return false;
}

static bool slatejs_transform_template_literals(const uint8_t *txt, size_t txtlen,
		char **out, size_t *outlen)
{
	const char *src = (const char *) txt;
	char *buf;
	size_t len = 0;
	size_t cap = txtlen * 2 + 64;
	size_t i = 0;
	char quote;

	*out = NULL;
	*outlen = 0;

	if (memchr(txt, '`', txtlen) == NULL) {
		return true;
	}

	buf = malloc(cap);
	if (buf == NULL) {
		return false;
	}
	buf[0] = '\0';

	while (i < txtlen) {
		char ch = src[i];

		if (ch == '\'' || ch == '"') {
			quote = ch;
			if (!slatejs_jsbuf_append_ch(&buf, &len, &cap, ch)) {
				goto error;
			}
			i++;
			while (i < txtlen) {
				ch = src[i];
				if (!slatejs_jsbuf_append_ch(&buf, &len, &cap, ch)) {
					goto error;
				}
				i++;
				if (ch == '\\' && i < txtlen) {
					if (!slatejs_jsbuf_append_ch(&buf, &len, &cap, src[i])) {
						goto error;
					}
					i++;
				} else if (ch == quote) {
					break;
				}
			}
			continue;
		}

		if (ch == '/' && i + 1 < txtlen && src[i + 1] == '/') {
			while (i < txtlen) {
				ch = src[i];
				if (!slatejs_jsbuf_append_ch(&buf, &len, &cap, ch)) {
					goto error;
				}
				i++;
				if (ch == '\n') {
					break;
				}
			}
			continue;
		}

		if (ch == '/' && i + 1 < txtlen && src[i + 1] == '*') {
			if (!slatejs_jsbuf_append(&buf, &len, &cap, "/*", 2)) {
				goto error;
			}
			i += 2;
			while (i < txtlen) {
				ch = src[i];
				if (!slatejs_jsbuf_append_ch(&buf, &len, &cap, ch)) {
					goto error;
				}
				i++;
				if (ch == '*' && i < txtlen && src[i] == '/') {
					if (!slatejs_jsbuf_append_ch(&buf, &len, &cap, src[i])) {
						goto error;
					}
					i++;
					break;
				}
			}
			continue;
		}

		if (ch == '`') {
			if (!slatejs_transform_template_literal(src, txtlen, &i, &buf, &len, &cap)) {
				goto error;
			}
			continue;
		}

		if (!slatejs_jsbuf_append_ch(&buf, &len, &cap, ch)) {
			goto error;
		}
		i++;
	}

	*out = buf;
	*outlen = len;
	return true;

error:
	free(buf);
	return false;
}

static void slatejs_dump_error(slatejs_context *ctx)
{
	/* stack is ..., errobj */
	slatejs_get_prop_string(ctx, -1, "name");
	slatejs_get_prop_string(ctx, -2, "message");
	slatejs_dup_top(ctx);
	/* ..., errobj, name, message, message */
	NSLOG(jserrors, WARNING, "Uncaught error in JS: %s: %s\n%s",
	      slatejs_safe_to_string(ctx, -3),
	      slatejs_safe_to_string(ctx, -2),
	      slatejs_safe_to_stacktrace(ctx, -1));
	/* ..., errobj, errobj.stackstring */
	slatejs_pop_3(ctx);
	/* ..., errobj */
}

static void slatejs_reset_start_time(slatejs_context *ctx)
{
	slatejs_memory_functions funcs;
	jsheap *heap;
	slatejs_get_memory_functions(ctx, &funcs);
	heap = funcs.udata;
	(void) nsu_getmonotonic_ms(&heap->exec_start_time);
}

slatejs_int_t slatejs_bind_pcall(slatejs_context *ctx, slatejs_size_t argc, bool reset_timeout)
{
	if (reset_timeout) {
		slatejs_reset_start_time(ctx);
	}

	slatejs_int_t ret = slatejs_pcall(ctx, argc);
	if (ret) {
		/* Something went wrong calling this... */
		slatejs_dump_error(ctx);
	}

	return ret;
}


void slatejs_bind_push_generics(slatejs_context *ctx, const char *generic)
{
	/* ... */
	slatejs_get_global_string(ctx, GENERICS_MAGIC);
	/* ..., generics */
	slatejs_get_prop_string(ctx, -1, generic);
	/* ..., generics, generic */
	slatejs_remove(ctx, -2);
	/* ..., generic */
}

static slatejs_int_t slatejs_bind_push_context_dump(slatejs_context *ctx, void *udata)
{
	slatejs_push_context_dump(ctx);
	return 1;
}

void slatejs_bind_log_stack_frame(slatejs_context *ctx, const char * reason)
{
	if (slatejs_safe_call(ctx, slatejs_bind_push_context_dump, NULL, 0, 1) != 0) {
		slatejs_pop(ctx);
		slatejs_push_string(ctx, "[???]");
	}
	NSLOG(quickjs, DEEPDEBUG, "%s, stack is: %s", reason, slatejs_safe_to_string(ctx, -1));
	slatejs_pop(ctx);
}


/* exported interface documented in js.h */
bool
js_exec(jsthread *thread, const uint8_t *txt, size_t txtlen, const char *name)
{
	bool ret = false;
	char *transformed = NULL;
	size_t transformed_len = 0;
	const char *compile_txt;
	size_t compile_len;
	assert(thread);

	if (txt == NULL || txtlen == 0) {
		return false;
	}

	if (thread->pending_destroy) {
		NSLOG(quickjs, DEEPDEBUG, "Skipping exec call because thread is dead");
		return false;
	}

	slatejs_enter_thread(thread);

	slatejs_set_top(CTX, 0);
	NSLOG(quickjs, DEEPDEBUG, "Running %"PRIsizet" bytes from %s", txtlen, name);
	/* NSLOG(quickjs, DEEPDEBUG, "\n%s\n", txt); */

	if (!slatejs_transform_template_literals(txt, txtlen, &transformed, &transformed_len)) {
		NSLOG(quickjs, DEBUG, "Failed to transform JavaScript template literals");
		goto out;
	}

	compile_txt = transformed != NULL ? transformed : (const char *) txt;
	compile_len = transformed != NULL ? transformed_len : txtlen;

	slatejs_reset_start_time(CTX);
	if (name != NULL) {
		slatejs_push_string(CTX, name);
	} else {
		slatejs_push_string(CTX, "?unknown source?");
	}
	if (slatejs_pcompile_lstring_filename(CTX,
					  SLATEJS_COMPILE_EVAL,
					  compile_txt,
					  compile_len) != 0) {
		NSLOG(quickjs, DEBUG, "Failed to compile JavaScript input");
		goto handle_error;
	}

	if (slatejs_pcall(CTX, 0/*nargs*/) == SLATEJS_EXEC_ERROR) {
		NSLOG(quickjs, DEBUG, "Failed to execute JavaScript");
		goto handle_error;
	}

	if (slatejs_get_top(CTX) == 0) slatejs_push_boolean(CTX, false);
	NSLOG(quickjs, DEEPDEBUG, "Returning %s",
	      slatejs_get_boolean(CTX, 0) ? "true" : "false");
	ret = slatejs_get_boolean(CTX, 0);
	goto out;

handle_error:
	slatejs_dump_error(CTX);
out:
	free(transformed);
	slatejs_leave_thread(thread);
	return ret;
}

static bool slatejs_event_type_is(dom_string *type, const char *name)
{
	size_t name_len;

	if (type == NULL) {
		return false;
	}

	name_len = strlen(name);
	return (dom_string_length(type) == name_len) &&
	       (memcmp(dom_string_data(type), name, name_len) == 0);
}

static const char* slatejs_event_proto(dom_event *evt)
{
	const char *ret = PROTO_NAME(EVENT);
	dom_string *type = NULL;
	dom_exception err;

	err = dom_event_get_type(evt, &type);
	if (err != DOM_NO_ERR) {
		goto out;
	}
	if (type == NULL) {
		goto out;
	}

	if (dom_string_isequal(type, corestring_dom_keydown)) {
		ret = PROTO_NAME(KEYBOARDEVENT);
		goto out;
	} else if (dom_string_isequal(type, corestring_dom_keyup)) {
		ret = PROTO_NAME(KEYBOARDEVENT);
		goto out;
	} else if (dom_string_isequal(type, corestring_dom_keypress)) {
		ret = PROTO_NAME(KEYBOARDEVENT);
		goto out;
	}

	if (slatejs_event_type_is(type, "click") ||
	    slatejs_event_type_is(type, "dblclick") ||
	    slatejs_event_type_is(type, "mousedown") ||
	    slatejs_event_type_is(type, "mouseup") ||
	    slatejs_event_type_is(type, "mousemove") ||
	    slatejs_event_type_is(type, "mouseover") ||
	    slatejs_event_type_is(type, "mouseout") ||
	    slatejs_event_type_is(type, "mouseenter") ||
	    slatejs_event_type_is(type, "mouseleave") ||
	    slatejs_event_type_is(type, "contextmenu")) {
		ret = PROTO_NAME(MOUSEEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "wheel")) {
		ret = PROTO_NAME(WHEELEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "drag") ||
	    slatejs_event_type_is(type, "dragstart") ||
	    slatejs_event_type_is(type, "dragenter") ||
	    slatejs_event_type_is(type, "dragover") ||
	    slatejs_event_type_is(type, "dragleave") ||
	    slatejs_event_type_is(type, "drop") ||
	    slatejs_event_type_is(type, "dragend")) {
		ret = PROTO_NAME(DRAGEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "focus") ||
	    slatejs_event_type_is(type, "blur") ||
	    slatejs_event_type_is(type, "focusin") ||
	    slatejs_event_type_is(type, "focusout")) {
		ret = PROTO_NAME(FOCUSEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "compositionstart") ||
	    slatejs_event_type_is(type, "compositionupdate") ||
	    slatejs_event_type_is(type, "compositionend")) {
		ret = PROTO_NAME(COMPOSITIONEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "storage")) {
		ret = PROTO_NAME(STORAGEEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "close")) {
		ret = PROTO_NAME(CLOSEEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "message")) {
		ret = PROTO_NAME(MESSAGEEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "error")) {
		ret = PROTO_NAME(ERROREVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "beforeunload")) {
		ret = PROTO_NAME(BEFOREUNLOADEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "pageshow") ||
	    slatejs_event_type_is(type, "pagehide")) {
		ret = PROTO_NAME(PAGETRANSITIONEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "hashchange")) {
		ret = PROTO_NAME(HASHCHANGEEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "popstate")) {
		ret = PROTO_NAME(POPSTATEEVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "autocompleteerror")) {
		ret = PROTO_NAME(AUTOCOMPLETEERROREVENT);
		goto out;
	}
	if (slatejs_event_type_is(type, "track")) {
		ret = PROTO_NAME(TRACKEVENT);
		goto out;
	}

out:
	if (type != NULL) {
		dom_string_unref(type);
	}

	return ret;
}

/*** New style event handling ***/

void slatejs_bind_push_event(slatejs_context *ctx, dom_event *evt)
{
	/* ... */
	slatejs_get_global_string(ctx, EVENT_MAGIC);
	/* ... events */
	slatejs_push_pointer(ctx, evt);
	/* ... events eventptr */
	slatejs_get_prop(ctx, -2);
	/* ... events event? */
	if (slatejs_is_undefined(ctx, -1)) {
		/* ... events undefined */
		slatejs_pop(ctx);
		/* ... events */
		slatejs_push_pointer(ctx, evt);
		if (slatejs_bind_create_object(ctx, slatejs_event_proto(evt), 1) != SLATEJS_EXEC_SUCCESS) {
			/* ... events err */
			slatejs_pop(ctx);
			/* ... events */
			slatejs_push_object(ctx);
			/* ... events eobj[meh] */
		}
		/* ... events eobj */
		slatejs_push_pointer(ctx, evt);
		/* ... events eobj eventptr */
		slatejs_dup(ctx, -2);
		/* ... events eobj eventptr eobj */
		slatejs_put_prop(ctx, -4);
		/* ... events eobj */
	}
	/* ... events event */
	slatejs_replace(ctx, -2);
	/* ... event */
}

static void slatejs_push_handler_code_(slatejs_context *ctx, dom_string *name,
				     dom_event_target *et)
{
	dom_string *onname, *val;
	dom_element *ele = (dom_element *)et;
	dom_exception exc;
	dom_node_type ntype;

	/* If et is NULL, then we're actually dealing with the Window object
	 * which has no default handlers and no way to assign handlers
	 * which aren't directly stored in the HANDLER_MAGIC
	 */
	if (et == NULL) {
		slatejs_push_lstring(ctx, "", 0);
		return;
	}

	/* The rest of this assumes et is a proper event target and expands
	 * out from there based on the assumption that all valid event targets
	 * are nodes.
	 */
	exc = dom_node_get_node_type(et, &ntype);
	if (exc != DOM_NO_ERR) {
		slatejs_push_lstring(ctx, "", 0);
		return;
	}

	if (ntype != DOM_ELEMENT_NODE) {
		slatejs_push_lstring(ctx, "", 0);
		return;
	}

	exc = dom_string_concat(corestring_dom_on, name, &onname);
	if (exc != DOM_NO_ERR) {
		slatejs_push_lstring(ctx, "", 0);
		return;
	}

	exc = dom_element_get_attribute(ele, onname, &val);
	if ((exc != DOM_NO_ERR) || (val == NULL)) {
		dom_string_unref(onname);
		slatejs_push_lstring(ctx, "", 0);
		return;
	}

	dom_string_unref(onname);
	slatejs_push_lstring(ctx, dom_string_data(val), dom_string_length(val));
	dom_string_unref(val);
}

bool slatejs_bind_get_current_value_of_event_handler(slatejs_context *ctx,
					      dom_string *name,
					      dom_event_target *et)
{
	/* Must be entered as:
	 * ... node(et)
	 */
	slatejs_get_prop_string(ctx, -1, HANDLER_MAGIC);
	/* ... node handlers */
	slatejs_push_lstring(ctx, dom_string_data(name), dom_string_length(name));
	/* ... node handlers name */
	slatejs_get_prop(ctx, -2);
	/* ... node handlers handler? */
	if (slatejs_is_undefined(ctx, -1)) {
		/* ... node handlers undefined */
		slatejs_pop_2(ctx);
		/* ... node */
		slatejs_push_handler_code_(ctx, name, et);
		/* ... node handlercode? */
		/* TODO: If this is null, clean up and propagate */
		/* ... node handlercode */
		/** @todo This is entirely wrong, but it's hard to get right */
		slatejs_push_string(ctx, "function (event) {");
		/* ... node handlercode prefix */
		slatejs_insert(ctx, -2);
		/* ... node prefix handlercode */
		slatejs_push_string(ctx, "}");
		/* ... node prefix handlercode suffix */
		slatejs_concat(ctx, 3);
		/* ... node fullhandlersrc */
		slatejs_push_string(ctx, "internal raw uncompiled handler");
		/* ... node fullhandlersrc filename */
		if (slatejs_pcompile(ctx, SLATEJS_COMPILE_FUNCTION) != 0) {
			/* ... node err */
			NSLOG(quickjs, DEBUG,
			      "Unable to proceed with handler, could not compile");
			slatejs_pop_2(ctx);
			return false;
		}
		/* ... node handler */
		slatejs_insert(ctx, -2);
		/* ... handler node */
	} else {
		/* ... node handlers handler */
		slatejs_insert(ctx, -3);
		/* ... handler node handlers */
		slatejs_pop(ctx);
		/* ... handler node */
	}
	/* ... handler node */
	return true;
}

static void slatejs_generic_event_handler(dom_event *evt, void *pw)
{
	slatejs_context *ctx = (slatejs_context *)pw;
	dom_string *name;
	dom_exception exc;
	dom_event_target *targ;
	dom_event_flow_phase phase;
	slatejs_uarridx_t idx;
	event_listener_flags flags;

	NSLOG(quickjs, DEBUG, "Handling an event in quickjs interface...");
	exc = dom_event_get_type(evt, &name);
	if (exc != DOM_NO_ERR) {
		NSLOG(quickjs, DEBUG, "Unable to find the event name");
		return;
	}
	NSLOG(quickjs, DEBUG, "Event's name is %*s", (int)dom_string_length(name),
	      dom_string_data(name));
	exc = dom_event_get_event_phase(evt, &phase);
	if (exc != DOM_NO_ERR) {
		NSLOG(quickjs, WARNING, "Unable to get event phase");
		return;
	}
	NSLOG(quickjs, DEBUG, "Event phase is: %s (%d)",
	      phase == DOM_CAPTURING_PHASE ? "capturing" : phase == DOM_AT_TARGET ? "at-target" : phase == DOM_BUBBLING_PHASE ? "bubbling" : "unknown",
	      (int)phase);

	exc = dom_event_get_current_target(evt, &targ);
	if (exc != DOM_NO_ERR) {
		dom_string_unref(name);
		NSLOG(quickjs, DEBUG, "Unable to find the event target");
		return;
	}

	/* If we're capturing right now, we skip the 'event handler'
	 * and go straight to the extras
	 */
	if (phase == DOM_CAPTURING_PHASE)
		goto handle_extras;

	/* ... */
	if (slatejs_bind_push_node(ctx, (dom_node *)targ) == false) {
		dom_string_unref(name);
		dom_node_unref(targ);
		NSLOG(quickjs, DEBUG,
		      "Unable to push JS node representation?!");
		return;
	}
	/* ... node */
	if (slatejs_bind_get_current_value_of_event_handler(
		    ctx, name, (dom_event_target *)targ) == false) {
		/* ... */
		goto handle_extras;
	}
	/* ... handler node */
	slatejs_bind_push_event(ctx, evt);
	/* ... handler node event */
	slatejs_reset_start_time(ctx);
	if (slatejs_pcall_method(ctx, 1) != 0) {
		/* Failed to run the method */
		/* ... err */
		NSLOG(quickjs, DEBUG,
		      "OH NOES! An error running a callback.  Meh.");
		exc = dom_event_stop_immediate_propagation(evt);
		if (exc != DOM_NO_ERR)
			NSLOG(quickjs, DEBUG,
			      "WORSE! could not stop propagation");
		slatejs_get_prop_string(ctx, -1, "name");
		slatejs_get_prop_string(ctx, -2, "message");
		slatejs_get_prop_string(ctx, -3, "fileName");
		slatejs_get_prop_string(ctx, -4, "lineNumber");
		slatejs_get_prop_string(ctx, -5, "stack");
		/* ... err name message fileName lineNumber stack */
		NSLOG(jserrors, WARNING, "JavaScript callback error: %s: %s",
		      slatejs_safe_to_string(ctx, -5),
		      slatejs_safe_to_string(ctx, -4));
		NSLOG(jserrors, WARNING, "JavaScript callback location: %s line %s",
		      slatejs_safe_to_string(ctx, -3),
		      slatejs_safe_to_string(ctx, -2));
		NSLOG(jserrors, WARNING, "JavaScript callback stack: %s",
		      slatejs_safe_to_string(ctx, -1));

		slatejs_pop_n(ctx, 6);
		/* ... */
		goto handle_extras;
	}
	/* ... result */
	if (slatejs_is_boolean(ctx, -1) &&
	    slatejs_to_boolean(ctx, -1) == 0) {
		dom_event_prevent_default(evt);
	}
	slatejs_pop(ctx);
handle_extras:
	/* ... */
	slatejs_push_lstring(ctx, dom_string_data(name), dom_string_length(name));
	slatejs_bind_push_node(ctx, (dom_node *)targ);
	/* ... type node */
	if (slatejs_bind_event_target_push_listeners(ctx, true)) {
		/* Nothing to do */
		slatejs_pop(ctx);
		goto out;
	}
	/* ... sublisteners */
	slatejs_push_array(ctx);
	/* ... sublisteners copy */
	idx = 0;
	while (slatejs_get_prop_index(ctx, -2, idx)) {
		/* ... sublisteners copy handler */
		slatejs_get_prop_index(ctx, -1, 1);
		/* ... sublisteners copy handler flags */
		if ((event_listener_flags)slatejs_to_int(ctx, -1) & ELF_ONCE) {
			slatejs_dup(ctx, -4);
			/* ... subl copy handler flags subl */
			slatejs_bind_shuffle_array(ctx, idx);
			slatejs_pop(ctx);
			/* ... subl copy handler flags */
		}
		slatejs_pop(ctx);
		/* ... sublisteners copy handler */
		slatejs_put_prop_index(ctx, -2, idx);
		/* ... sublisteners copy */
		idx++;
	}
	/* ... sublisteners copy undefined */
	slatejs_pop(ctx);
	/* ... sublisteners copy */
	slatejs_insert(ctx, -2);
	/* ... copy sublisteners */
	slatejs_pop(ctx);
	/* ... copy */
	idx = 0;
	while (slatejs_get_prop_index(ctx, -1, idx++)) {
		/* ... copy handler */
		if (slatejs_get_prop_index(ctx, -1, 2)) {
			/* ... copy handler meh */
			slatejs_pop_2(ctx);
			continue;
		}
		slatejs_pop(ctx);
		slatejs_get_prop_index(ctx, -1, 0);
		slatejs_get_prop_index(ctx, -2, 1);
		/* ... copy handler callback flags */
		flags = (event_listener_flags)slatejs_get_int(ctx, -1);
		slatejs_pop(ctx);
		/* ... copy handler callback */
		if (((phase == DOM_CAPTURING_PHASE) && !(flags & ELF_CAPTURE)) ||
		    ((phase != DOM_CAPTURING_PHASE) && (flags & ELF_CAPTURE))) {
			slatejs_pop_2(ctx);
			/* ... copy */
			continue;
		}
		/* ... copy handler callback */
		slatejs_bind_push_node(ctx, (dom_node *)targ);
		/* ... copy handler callback node */
		slatejs_bind_push_event(ctx, evt);
		/* ... copy handler callback node event */
		slatejs_reset_start_time(ctx);
		if (slatejs_pcall_method(ctx, 1) != 0) {
			/* Failed to run the method */
			/* ... copy handler err */
			NSLOG(quickjs, DEBUG,
			      "OH NOES! An error running a callback.  Meh.");
			exc = dom_event_stop_immediate_propagation(evt);
			if (exc != DOM_NO_ERR)
				NSLOG(quickjs, DEBUG,
				      "WORSE! could not stop propagation");
			slatejs_get_prop_string(ctx, -1, "name");
			slatejs_get_prop_string(ctx, -2, "message");
			slatejs_get_prop_string(ctx, -3, "fileName");
			slatejs_get_prop_string(ctx, -4, "lineNumber");
			slatejs_get_prop_string(ctx, -5, "stack");
			/* ... err name message fileName lineNumber stack */
			NSLOG(quickjs, DEBUG, "Uncaught error in JS: %s: %s",
			      slatejs_safe_to_string(ctx, -5),
			      slatejs_safe_to_string(ctx, -4));
			NSLOG(quickjs, DEBUG,
			      "              was at: %s line %s",
			      slatejs_safe_to_string(ctx, -3),
			      slatejs_safe_to_string(ctx, -2));
			NSLOG(quickjs, DEBUG, "         Stack trace: %s",
			      slatejs_safe_to_string(ctx, -1));

			slatejs_pop_n(ctx, 7);
			/* ... copy */
			continue;
		}
		/* ... copy handler result */
		if (slatejs_is_boolean(ctx, -1) &&
		    slatejs_to_boolean(ctx, -1) == 0) {
			dom_event_prevent_default(evt);
		}
		slatejs_pop_2(ctx);
		/* ... copy */
	}
	slatejs_pop_2(ctx);
out:
	/* ... */
	dom_node_unref(targ);
	dom_string_unref(name);
}

void slatejs_bind_register_event_listener_for(slatejs_context *ctx,
				       struct dom_element *ele,
				       dom_string *name,
				       bool capture)
{
	dom_event_listener *listen = NULL;
	dom_exception exc;

	/* ... */
	if (ele == NULL) {
		/* A null element is the Window object */
		slatejs_push_global_object(ctx);
	} else {
		/* Non null elements must be pushed as a node object */
		if (slatejs_bind_push_node(ctx, (struct dom_node *)ele) == false)
			return;
	}
	/* ... node */
	slatejs_get_prop_string(ctx, -1, HANDLER_LISTENER_MAGIC);
	/* ... node handlers */
	slatejs_push_lstring(ctx, dom_string_data(name), dom_string_length(name));
	/* ... node handlers name */
	if (slatejs_has_prop(ctx, -2)) {
		/* ... node handlers */
		slatejs_pop_2(ctx);
		/* ... */
		return;
	}
	/* ... node handlers */
	slatejs_push_lstring(ctx, dom_string_data(name), dom_string_length(name));
	/* ... node handlers name */
	slatejs_push_boolean(ctx, true);
	/* ... node handlers name true */
	slatejs_put_prop(ctx, -3);
	/* ... node handlers */
	slatejs_pop_2(ctx);
	/* ... */
	if (ele == NULL) {
		/* Nothing more to do, Window doesn't register in the
		 * normal event listener flow
		 */
		return;
	}

	/* Otherwise add an event listener to the element */
	exc = dom_event_listener_create(slatejs_generic_event_handler, ctx,
					&listen);
	if (exc != DOM_NO_ERR) return;
	exc = dom_event_target_add_event_listener(
		ele, name, listen, capture);
	if (exc != DOM_NO_ERR) {
		NSLOG(quickjs, DEBUG,
		      "Unable to register listener for %p.%*s", ele,
		      (int)dom_string_length(name), dom_string_data(name));
	} else {
		NSLOG(quickjs, DEBUG, "have registered listener for %p.%*s",
		      ele, (int)dom_string_length(name), dom_string_data(name));
	}
	dom_event_listener_unref(listen);
}

/* The sub-listeners are a list of {callback,flags} tuples */
/* We return true if we created a new sublistener table */
/* If we're told to not create, but we want to, we still return true */
bool slatejs_bind_event_target_push_listeners(slatejs_context *ctx, bool dont_create)
{
	bool ret = false;
	/* ... type this */
	slatejs_get_prop_string(ctx, -1, EVENT_LISTENER_JS_MAGIC);
	if (slatejs_is_undefined(ctx, -1)) {
		/* ... type this null */
		slatejs_pop(ctx);
		slatejs_push_object(ctx);
		slatejs_dup(ctx, -1);
		/* ... type this listeners listeners */
		slatejs_put_prop_string(ctx, -3, EVENT_LISTENER_JS_MAGIC);
		/* ... type this listeners */
	}
	/* ... type this listeners */
	slatejs_insert(ctx, -3);
	/* ... listeners type this */
	slatejs_pop(ctx);
	/* ... listeners type */
	slatejs_dup(ctx, -1);
	/* ... listeners type type */
	slatejs_get_prop(ctx, -3);
	/* ... listeners type ??? */
	if (slatejs_is_undefined(ctx, -1)) {
		/* ... listeners type ??? */
		if (dont_create == true) {
			slatejs_pop_3(ctx);
			slatejs_push_undefined(ctx);
			return true;
		}
		slatejs_pop(ctx);
		slatejs_push_array(ctx);
		slatejs_dup(ctx, -2);
		slatejs_dup(ctx, -2);
		/* ... listeners type sublisteners type sublisteners */
		slatejs_put_prop(ctx, -5);
		/* ... listeners type sublisteners */
		ret = true;
	}
	slatejs_insert(ctx, -3);
	/* ... sublisteners listeners type */
	slatejs_pop_2(ctx);
	/* ... sublisteners */
	return ret;
}

/* Shuffle a quickjs array "down" one.  This involves iterating from
 * the index provided, shuffling elements down, until we reach an
 * undefined
 */
void slatejs_bind_shuffle_array(slatejs_context *ctx, slatejs_uarridx_t idx)
{
	/* ... somearr */
	while (slatejs_get_prop_index(ctx, -1, idx + 1)) {
		slatejs_put_prop_index(ctx, -2, idx);
		idx++;
	}
	/* ... somearr undefined */
	slatejs_del_prop_index(ctx, -2, idx + 1);
	slatejs_pop(ctx);
}


void js_handle_new_element(jsthread *thread, struct dom_element *node)
{
	assert(thread);
	assert(node);
	dom_namednodemap *map;
	dom_exception exc;
	dom_ulong idx;
	dom_ulong siz;
	dom_attr *attr = NULL;
	dom_string *key = NULL;
	dom_string *nodename;
	slatejs_bool_t is_body = false;

	exc = dom_node_get_node_name(node, &nodename);
	if (exc != DOM_NO_ERR) return;

	if (nodename == corestring_dom_BODY)
		is_body = true;

	dom_string_unref(nodename);

	exc = dom_node_get_attributes(node, &map);
	if (exc != DOM_NO_ERR) return;
	if (map == NULL) return;

	slatejs_enter_thread(thread);

	exc = dom_namednodemap_get_length(map, &siz);
	if (exc != DOM_NO_ERR) goto out;

	for (idx = 0; idx < siz; idx++) {
		exc = dom_namednodemap_item(map, idx, &attr);
		if (exc != DOM_NO_ERR) goto out;
		exc = dom_attr_get_name(attr, &key);
		if (exc != DOM_NO_ERR) goto out;
		if (is_body && (
			    key == corestring_dom_onblur ||
			    key == corestring_dom_onerror ||
			    key == corestring_dom_onfocus ||
			    key == corestring_dom_onload ||
			    key == corestring_dom_onresize ||
			    key == corestring_dom_onscroll)) {
			/* This is a forwarded event, it doesn't matter,
			 * we should skip registering for it and later
			 * we will register it for Window itself
			 */
			goto skip_register;
		}
		if (dom_string_length(key) > 2) {
			/* Can be on* */
			const uint8_t *data = (const uint8_t *)dom_string_data(key);
			if (data[0] == 'o' && data[1] == 'n') {
				dom_string *sub = NULL;
				exc = dom_string_substr(
					key, 2, dom_string_length(key),
					&sub);
				if (exc == DOM_NO_ERR) {
					slatejs_bind_register_event_listener_for(
						CTX, node, sub, false);
					dom_string_unref(sub);
				}
			}
		}
	skip_register:
		dom_string_unref(key); key = NULL;
		dom_node_unref(attr); attr = NULL;
	}

out:
	if (key != NULL)
		dom_string_unref(key);

	if (attr != NULL)
		dom_node_unref(attr);

	dom_namednodemap_unref(map);

	slatejs_leave_thread(thread);
}

void js_event_cleanup(jsthread *thread, struct dom_event *evt)
{
	assert(thread);
	slatejs_enter_thread(thread);
	/* ... */
	slatejs_get_global_string(CTX, EVENT_MAGIC);
	/* ... EVENT_MAP */
	slatejs_push_pointer(CTX, evt);
	/* ... EVENT_MAP eventptr */
	slatejs_del_prop(CTX, -2);
	/* ... EVENT_MAP */
	slatejs_pop(CTX);
	/* ... */
	slatejs_leave_thread(thread);
}

bool js_fire_event(jsthread *thread, const char *type, struct dom_document *doc, struct dom_node *target)
{
	dom_exception exc;
	dom_event *evt;
	dom_event_target *body;

	NSLOG(quickjs, DEBUG, "Event: %s (doc=%p, target=%p)", type, doc,
	      target);

	/** @todo Make this more generic, this only handles load and only
	 * targetting the window, so that we actually stand a chance of
	 * getting 3.4 out.
	 */

	if (target != NULL)
		/* Swallow non-Window-targetted events quietly */
		return true;

	if (strcmp(type, "load") != 0)
		/* Swallow non-load events quietly */
		return true;

	/* Okay, we're processing load, targetted at Window, do the single
	 * thing which gets us there, which is to find the appropriate event
	 * handler and call it.  If we have no event handler on Window then
	 * we divert to the body, and if there's no event handler there
	 * we swallow the event silently
	 */

	exc = dom_event_create(&evt);
	if (exc != DOM_NO_ERR) return true;
	exc = dom_event_init(evt, corestring_dom_load, false, false);
	if (exc != DOM_NO_ERR) {
		dom_event_unref(evt);
		return true;
	}
	slatejs_enter_thread(thread);
	/* ... */
	slatejs_get_global_string(CTX, HANDLER_MAGIC);
	/* ... handlers */
	slatejs_push_lstring(CTX, "load", 4);
	/* ... handlers "load" */
	slatejs_get_prop(CTX, -2);
	/* ... handlers handler? */
	if (slatejs_is_undefined(CTX, -1)) {
		/* No handler here, *try* and retrieve a handler from
		 * the body
		 */
		slatejs_pop(CTX);
		/* ... handlers */
		exc = dom_html_document_get_body(doc, &body);
		if (exc != DOM_NO_ERR) {
			dom_event_unref(evt);
			slatejs_leave_thread(thread);
			return true;
		}
		slatejs_bind_push_node(CTX, (struct dom_node *)body);
		/* ... handlers bodynode */
		if (slatejs_bind_get_current_value_of_event_handler(
			    CTX, corestring_dom_load, body) == false) {
			/* Unref the body, we don't need it any more */
			dom_node_unref(body);
			/* ... handlers */
			slatejs_pop(CTX);
			slatejs_leave_thread(thread);
			return true;
		}
		/* Unref the body, we don't need it any more */
		dom_node_unref(body);
		/* ... handlers handler bodynode */
		slatejs_pop(CTX);
	}
	/* ... handlers handler */
	slatejs_insert(CTX, -2);
	/* ... handler handlers */
	slatejs_pop(CTX);
	/* ... handler */
	slatejs_push_global_object(CTX);
	/* ... handler Window */
	slatejs_bind_push_event(CTX, evt);
	/* ... handler Window event */
	slatejs_reset_start_time(CTX);
	if (slatejs_pcall_method(CTX, 1) != 0) {
		/* Failed to run the handler */
		/* ... err */
		NSLOG(quickjs, DEBUG,
		      "OH NOES! An error running a handler.  Meh.");
		slatejs_get_prop_string(CTX, -1, "name");
		slatejs_get_prop_string(CTX, -2, "message");
		slatejs_get_prop_string(CTX, -3, "fileName");
		slatejs_get_prop_string(CTX, -4, "lineNumber");
		slatejs_get_prop_string(CTX, -5, "stack");
		/* ... err name message fileName lineNumber stack */
		NSLOG(quickjs, DEBUG, "Uncaught error in JS: %s: %s",
		      slatejs_safe_to_string(CTX, -5),
		      slatejs_safe_to_string(CTX, -4));
		NSLOG(quickjs, DEBUG, "              was at: %s line %s",
		      slatejs_safe_to_string(CTX, -3),
		      slatejs_safe_to_string(CTX, -2));
		NSLOG(quickjs, DEBUG, "         Stack trace: %s",
		      slatejs_safe_to_string(CTX, -1));

		slatejs_pop_n(CTX, 6);
		/* ... */
		js_event_cleanup(thread, evt);
		dom_event_unref(evt);
		slatejs_leave_thread(thread);
		return true;
	}
	/* ... result */
	slatejs_pop(CTX);
	/* ... */
	js_event_cleanup(thread, evt);
	dom_event_unref(evt);
	slatejs_leave_thread(thread);
	return true;
}
