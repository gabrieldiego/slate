/*
 * QuickJS-backed subset of Duktape used by Slate's generated JavaScript
 * bindings.
 *
 * TODO: Replace this migration shim with slategenbind output that talks to the
 * QuickJS C API directly. The shim exists only to get a functional,
 * measurable engine switch before reworking the binding generator.
 */

#include <assert.h>
#include <inttypes.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(__linux__)
#include <malloc.h>
#endif

#include "duktape.h"
#include "quickjs.h"

extern duk_bool_t dukky_check_timeout(void *udata);

struct duk_string_cache {
	char *data;
	duk_size_t len;
	struct duk_string_cache *next;
};

struct duk_cfunc_ref {
	duk_c_function func;
	duk_idx_t nargs;
};

struct duk_object_meta {
	duk_context *ctx;
	duk_c_function finalizer;
	void *private_ptr;
	struct duk_object_meta *next_finalizer;
};

struct duk_pointer_ref {
	void *ptr;
};

struct duk_context {
	JSRuntime *rt;
	JSContext *qctx;
	JSValue *stack;
	duk_idx_t top;
	duk_idx_t cap;
	JSValue global_object;
	JSValue current_this;
	duk_memory_functions memfuncs;
	char *pending_error;
	struct duk_string_cache *strings;
	struct duk_context *parent;
	struct duk_context *children;
	struct duk_context *next_child;
	struct duk_object_meta *finalizer_queue;
	struct duk_object_meta *finalizer_queue_tail;
	void *finalizer_private_ptr;
	bool draining_finalizers;
	bool owns_runtime;
};

static JSClassID duk_object_class_id;
static JSClassID duk_cfunc_class_id;
static JSClassID duk_pointer_class_id;
static JSClassID duk_thread_class_id;
static bool duk_class_ids_ready;
static const char duk_cfunc_ref_prop[] = "\xffSlateDuktapeCFunctionRef";
static const char duk_private_magic_prop[] = "\xff\xffNETSURF_DUKTAPE_PRIVATE";

static void duk_object_finalizer(JSRuntime *rt, JSValue val);
static void duk_cfunc_finalizer(JSRuntime *rt, JSValue val);
static JSValue duk_cfunc_call(JSContext *ctx, JSValueConst func_obj,
		JSValueConst this_val, int argc, JSValueConst *argv,
		int flags);
static JSValue duk_cfunc_data_call(JSContext *ctx, JSValueConst this_val,
		int argc, JSValueConst *argv, int magic, JSValue *func_data);
static struct duk_cfunc_ref *duk_get_cfunc_ref(duk_context *ctx,
		JSValueConst val);
static void duk_pointer_finalizer(JSRuntime *rt, JSValue val);
static JSValue duk_get_effective_global(duk_context *ctx);
static bool duk_is_effective_global_value(duk_context *ctx, JSValueConst val);
static void duk_sync_qjs_global(duk_context *ctx);
static void duk_drain_finalizers(duk_context *ctx);
static JSValue duk_call_c_function_value(duk_context *ctx,
		duk_c_function func, JSValueConst this_val, int argc,
		JSValueConst *argv);

static const JSClassDef duk_object_class = {
	.class_name = "SlateDuktapeObject",
	.finalizer = duk_object_finalizer,
};

static const JSClassDef duk_cfunc_class = {
	.class_name = "SlateDuktapeCFunction",
	.finalizer = duk_cfunc_finalizer,
	.call = duk_cfunc_call,
};

static const JSClassDef duk_pointer_class = {
	.class_name = "SlateDuktapePointer",
	.finalizer = duk_pointer_finalizer,
};

static const JSClassDef duk_thread_class = {
	.class_name = "SlateDuktapeThread",
};

static void duk_init_class_ids(void)
{
	if (duk_class_ids_ready) {
		return;
	}

	JS_NewClassID(&duk_object_class_id);
	JS_NewClassID(&duk_cfunc_class_id);
	JS_NewClassID(&duk_pointer_class_id);
	JS_NewClassID(&duk_thread_class_id);
	duk_class_ids_ready = true;
}

static void duk_set_default_class_proto(JSContext *ctx, JSClassID class_id)
{
	JSValue obj = JS_NewObject(ctx);
	JSValue proto = JS_GetPrototype(ctx, obj);

	JS_FreeValue(ctx, obj);
	if (!JS_IsException(proto)) {
		JS_SetClassProto(ctx, class_id, proto);
	}
}

static bool duk_register_classes(JSRuntime *rt, JSContext *ctx)
{
	duk_init_class_ids();

	if (!JS_IsRegisteredClass(rt, duk_object_class_id) &&
			JS_NewClass(rt, duk_object_class_id, &duk_object_class) < 0) {
		return false;
	}
	if (!JS_IsRegisteredClass(rt, duk_cfunc_class_id) &&
			JS_NewClass(rt, duk_cfunc_class_id, &duk_cfunc_class) < 0) {
		return false;
	}
	if (!JS_IsRegisteredClass(rt, duk_pointer_class_id) &&
			JS_NewClass(rt, duk_pointer_class_id, &duk_pointer_class) < 0) {
		return false;
	}
	if (!JS_IsRegisteredClass(rt, duk_thread_class_id) &&
			JS_NewClass(rt, duk_thread_class_id, &duk_thread_class) < 0) {
		return false;
	}

	duk_set_default_class_proto(ctx, duk_object_class_id);
	duk_set_default_class_proto(ctx, duk_cfunc_class_id);
	duk_set_default_class_proto(ctx, duk_pointer_class_id);
	duk_set_default_class_proto(ctx, duk_thread_class_id);

	return true;
}

static void duk_clear_strings(duk_context *ctx)
{
	struct duk_string_cache *cur = ctx->strings;

	while (cur != NULL) {
		struct duk_string_cache *next = cur->next;
		free(cur->data);
		free(cur);
		cur = next;
	}
	ctx->strings = NULL;
}

static const char *duk_cache_string(duk_context *ctx, const char *data,
		duk_size_t len)
{
	struct duk_string_cache *cache;

	cache = calloc(1, sizeof(*cache));
	if (cache == NULL) {
		return "";
	}
	cache->data = malloc(len + 1);
	if (cache->data == NULL) {
		free(cache);
		return "";
	}
	memcpy(cache->data, data, len);
	cache->data[len] = '\0';
	cache->len = len;
	cache->next = ctx->strings;
	ctx->strings = cache;
	return cache->data;
}

static const char *duk_value_to_cached_string(duk_context *ctx, JSValueConst val,
		duk_size_t *out_len, bool safe)
{
	const char *qstr;
	const char *ret;
	size_t len = 0;

	qstr = JS_ToCStringLen(ctx->qctx, &len, val);
	if (qstr == NULL) {
		JSValue exc = JS_GetException(ctx->qctx);
		JS_FreeValue(ctx->qctx, exc);
		if (!safe) {
			return NULL;
		}
		ret = duk_cache_string(ctx, "[conversion error]",
				sizeof("[conversion error]") - 1);
		if (out_len != NULL) {
			*out_len = sizeof("[conversion error]") - 1;
		}
		return ret;
	}

	ret = duk_cache_string(ctx, qstr, len);
	JS_FreeCString(ctx->qctx, qstr);
	if (out_len != NULL) {
		*out_len = len;
	}
	return ret;
}

static bool duk_reserve_stack(duk_context *ctx, duk_idx_t needed)
{
	JSValue *nstack;
	duk_idx_t ncap;

	if (needed <= ctx->cap) {
		return true;
	}

	ncap = ctx->cap == 0 ? 16 : ctx->cap;
	while (ncap < needed) {
		ncap *= 2;
	}

	nstack = realloc(ctx->stack, sizeof(*nstack) * ncap);
	if (nstack == NULL) {
		return false;
	}

	ctx->stack = nstack;
	ctx->cap = ncap;
	return true;
}

static bool duk_push_owned(duk_context *ctx, JSValue val)
{
	if (!duk_reserve_stack(ctx, ctx->top + 1)) {
		JS_FreeValue(ctx->qctx, val);
		return false;
	}

	ctx->stack[ctx->top++] = val;
	return true;
}

static duk_idx_t duk_resolve_index(duk_context *ctx, duk_idx_t idx)
{
	if (idx < 0) {
		idx = ctx->top + idx;
	}
	if (idx < 0 || idx >= ctx->top) {
		return -1;
	}
	return idx;
}

static JSValue duk_value_at(duk_context *ctx, duk_idx_t idx)
{
	idx = duk_resolve_index(ctx, idx);
	if (idx < 0) {
		return JS_UNDEFINED;
	}
	return ctx->stack[idx];
}

static JSValue *duk_value_slot(duk_context *ctx, duk_idx_t idx)
{
	idx = duk_resolve_index(ctx, idx);
	if (idx < 0) {
		return NULL;
	}
	return &ctx->stack[idx];
}

static void duk_free_range(duk_context *ctx, duk_idx_t start, duk_idx_t count)
{
	duk_idx_t i;

	if (count <= 0) {
		return;
	}

	for (i = start; i < start + count; i++) {
		JS_FreeValue(ctx->qctx, ctx->stack[i]);
	}
	memmove(&ctx->stack[start], &ctx->stack[start + count],
			sizeof(*ctx->stack) * (ctx->top - start - count));
	ctx->top -= count;
}

static int duk_qjs_type_to_duk(JSValueConst val)
{
	struct duk_pointer_ref *pref;

	if (JS_IsUndefined(val)) {
		return DUK_TYPE_UNDEFINED;
	}
	if (JS_IsNull(val)) {
		return DUK_TYPE_NULL;
	}
	if (JS_IsBool(val)) {
		return DUK_TYPE_BOOLEAN;
	}
	if (JS_IsNumber(val)) {
		return DUK_TYPE_NUMBER;
	}
	if (JS_IsString(val)) {
		return DUK_TYPE_STRING;
	}
	pref = JS_GetOpaque(val, duk_pointer_class_id);
	if (pref != NULL) {
		return DUK_TYPE_POINTER;
	}
	if (JS_IsObject(val)) {
		return DUK_TYPE_OBJECT;
	}
	return DUK_TYPE_NONE;
}

static void duk_set_pending_errorv(duk_context *ctx, const char *fmt, va_list ap)
{
	va_list copy;
	int len;

	free(ctx->pending_error);
	ctx->pending_error = NULL;

	va_copy(copy, ap);
	len = vsnprintf(NULL, 0, fmt, copy);
	va_end(copy);
	if (len < 0) {
		return;
	}

	ctx->pending_error = malloc((size_t)len + 1);
	if (ctx->pending_error == NULL) {
		return;
	}
	vsnprintf(ctx->pending_error, (size_t)len + 1, fmt, ap);
}

static JSValue duk_throw_pending(duk_context *ctx)
{
	const char *msg = ctx->pending_error != NULL ?
			ctx->pending_error : "Duktape compatibility error";
	return JS_ThrowTypeError(ctx->qctx, "%s", msg);
}

static JSValue duk_make_error(duk_context *ctx, const char *msg)
{
	JSValue err = JS_NewError(ctx->qctx);

	if (JS_IsException(err)) {
		return JS_GetException(ctx->qctx);
	}

	JS_DefinePropertyValueStr(ctx->qctx, err, "message",
			JS_NewString(ctx->qctx, msg != NULL ? msg : ""),
			JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
	return err;
}

static JSAtom duk_atom_from_value(duk_context *ctx, JSValueConst key)
{
	struct duk_pointer_ref *pref = JS_GetOpaque(key, duk_pointer_class_id);

	if (pref != NULL) {
		char name[64];
		snprintf(name, sizeof(name), "\xffSLATE_PTR_%p", pref->ptr);
		return JS_NewAtom(ctx->qctx, name);
	}

	return JS_ValueToAtom(ctx->qctx, key);
}

static int duk_qjs_interrupt(JSRuntime *rt, void *opaque)
{
	duk_context *ctx = opaque;

	if (ctx == NULL) {
		return 0;
	}

	return dukky_check_timeout(ctx->memfuncs.udata) ? 1 : 0;
}

static void *duk_qjs_malloc(JSMallocState *s, size_t size)
{
	duk_context *ctx = s->opaque;

	if (ctx->memfuncs.alloc_func != NULL) {
		return ctx->memfuncs.alloc_func(ctx->memfuncs.udata, size);
	}
	return malloc(size);
}

static void duk_qjs_free(JSMallocState *s, void *ptr)
{
	duk_context *ctx = s->opaque;

	if (ctx->memfuncs.free_func != NULL) {
		ctx->memfuncs.free_func(ctx->memfuncs.udata, ptr);
		return;
	}
	free(ptr);
}

static void *duk_qjs_realloc(JSMallocState *s, void *ptr, size_t size)
{
	duk_context *ctx = s->opaque;

	if (ctx->memfuncs.realloc_func != NULL) {
		return ctx->memfuncs.realloc_func(ctx->memfuncs.udata, ptr, size);
	}
	return realloc(ptr, size);
}

static size_t duk_qjs_malloc_usable_size(const void *ptr)
{
#if defined(__linux__)
	return ptr != NULL ? malloc_usable_size((void *)ptr) : 0;
#else
	(void)ptr;
	return 0;
#endif
}

static const JSMallocFunctions duk_qjs_malloc_funcs = {
	.js_malloc = duk_qjs_malloc,
	.js_free = duk_qjs_free,
	.js_realloc = duk_qjs_realloc,
	.js_malloc_usable_size = duk_qjs_malloc_usable_size,
};

duk_context *duk_create_heap(duk_alloc_function alloc_func,
		duk_realloc_function realloc_func,
		duk_free_function free_func,
		void *heap_udata,
		duk_fatal_function fatal_handler)
{
	duk_context *ctx;

	(void)fatal_handler;

	ctx = calloc(1, sizeof(*ctx));
	if (ctx == NULL) {
		return NULL;
	}

	ctx->memfuncs.alloc_func = alloc_func;
	ctx->memfuncs.realloc_func = realloc_func;
	ctx->memfuncs.free_func = free_func;
	ctx->memfuncs.udata = heap_udata;
	ctx->global_object = JS_UNDEFINED;
	ctx->current_this = JS_UNDEFINED;
	ctx->owns_runtime = true;

	ctx->rt = JS_NewRuntime2(&duk_qjs_malloc_funcs, ctx);
	if (ctx->rt == NULL) {
		free(ctx);
		return NULL;
	}

	JS_SetRuntimeOpaque(ctx->rt, ctx);
	JS_SetCanBlock(ctx->rt, 0);
	JS_SetInterruptHandler(ctx->rt, duk_qjs_interrupt, ctx);

	ctx->qctx = JS_NewContext(ctx->rt);
	if (ctx->qctx == NULL) {
		JS_FreeRuntime(ctx->rt);
		free(ctx);
		return NULL;
	}
	JS_SetContextOpaque(ctx->qctx, ctx);

	if (!duk_register_classes(ctx->rt, ctx->qctx)) {
		JS_FreeContext(ctx->qctx);
		JS_FreeRuntime(ctx->rt);
		free(ctx);
		return NULL;
	}

	return ctx;
}

static void duk_free_context_values(duk_context *ctx)
{
	duk_idx_t i;

	for (i = 0; i < ctx->top; i++) {
		JS_FreeValue(ctx->qctx, ctx->stack[i]);
	}
	free(ctx->stack);
	ctx->stack = NULL;
	ctx->top = 0;
	ctx->cap = 0;
	JS_FreeValue(ctx->qctx, ctx->current_this);
	ctx->current_this = JS_UNDEFINED;
	duk_clear_strings(ctx);
	free(ctx->pending_error);
	ctx->pending_error = NULL;
}

static void duk_free_child_contexts(duk_context *ctx)
{
	struct duk_context *child = ctx->children;

	while (child != NULL) {
		struct duk_context *next = child->next_child;
		duk_free_child_contexts(child);
		duk_free_context_values(child);
		duk_drain_finalizers(child);
		JS_FreeValue(child->qctx, child->global_object);
		child->global_object = JS_UNDEFINED;
		duk_drain_finalizers(child);
		child = next;
	}
}

static void duk_free_child_context_structs(duk_context *ctx)
{
	struct duk_context *child = ctx->children;

	while (child != NULL) {
		struct duk_context *next = child->next_child;
		duk_free_child_context_structs(child);
		free(child);
		child = next;
	}
	ctx->children = NULL;
}

static JSValue duk_get_effective_global(duk_context *ctx)
{
	if (!JS_IsUndefined(ctx->global_object)) {
		return JS_DupValue(ctx->qctx, ctx->global_object);
	}

	return JS_GetGlobalObject(ctx->qctx);
}

static bool duk_is_effective_global_value(duk_context *ctx, JSValueConst val)
{
	return !JS_IsUndefined(ctx->global_object) &&
		JS_IsObject(val) &&
		JS_VALUE_GET_TAG(val) == JS_VALUE_GET_TAG(ctx->global_object) &&
		JS_VALUE_GET_PTR(val) == JS_VALUE_GET_PTR(ctx->global_object);
}

static void duk_sync_qjs_global(duk_context *ctx)
{
	JSValue global;

	JS_SetContextOpaque(ctx->qctx, ctx);
	if (JS_IsUndefined(ctx->global_object)) {
		return;
	}

	global = JS_GetGlobalObject(ctx->qctx);
	JS_SetPrototype(ctx->qctx, global, ctx->global_object);
	JS_FreeValue(ctx->qctx, global);
}

static duk_context *duk_root_context(duk_context *ctx)
{
	while (ctx != NULL && ctx->parent != NULL) {
		ctx = ctx->parent;
	}
	return ctx;
}

static void duk_queue_finalizer(struct duk_object_meta *meta)
{
	duk_context *root;

	if (meta == NULL) {
		return;
	}
	if (meta->ctx == NULL || meta->finalizer == NULL ||
			meta->private_ptr == NULL) {
		free(meta);
		return;
	}

	root = duk_root_context(meta->ctx);
	if (root == NULL) {
		free(meta);
		return;
	}

	meta->next_finalizer = NULL;
	if (root->finalizer_queue_tail != NULL) {
		root->finalizer_queue_tail->next_finalizer = meta;
	} else {
		root->finalizer_queue = meta;
	}
	root->finalizer_queue_tail = meta;
}

static void duk_drain_finalizers(duk_context *ctx)
{
	duk_context *root = duk_root_context(ctx);

	if (root == NULL || root->draining_finalizers) {
		return;
	}

	root->draining_finalizers = true;
	while (root->finalizer_queue != NULL) {
		struct duk_object_meta *meta = root->finalizer_queue;
		duk_context *owner = meta->ctx;
		void *old_private;
		void *old_opaque;
		JSValue arg = JS_UNDEFINED;
		JSValue result;

		root->finalizer_queue = meta->next_finalizer;
		if (root->finalizer_queue == NULL) {
			root->finalizer_queue_tail = NULL;
		}
		meta->next_finalizer = NULL;

		old_private = owner->finalizer_private_ptr;
		old_opaque = JS_GetContextOpaque(owner->qctx);
		owner->finalizer_private_ptr = meta->private_ptr;
		JS_SetContextOpaque(owner->qctx, owner);
		result = duk_call_c_function_value(owner, meta->finalizer,
				JS_UNDEFINED, 1, &arg);
		if (JS_IsException(result)) {
			JS_FreeValue(owner->qctx, JS_GetException(owner->qctx));
		} else {
			JS_FreeValue(owner->qctx, result);
		}
		JS_SetContextOpaque(owner->qctx, old_opaque);
		owner->finalizer_private_ptr = old_private;

		free(meta);
	}
	root->draining_finalizers = false;
}

static void duk_collect_and_drain(duk_context *ctx)
{
	duk_context *root = duk_root_context(ctx);
	int i;

	if (root == NULL) {
		return;
	}

	for (i = 0; i < 4; i++) {
		JS_RunGC(root->rt);
		duk_drain_finalizers(root);
		if (root->finalizer_queue == NULL) {
			break;
		}
	}
}

static void duk_detach_qjs_global(duk_context *ctx)
{
	JSValue global;

	if (ctx == NULL || ctx->qctx == NULL) {
		return;
	}

	global = JS_GetGlobalObject(ctx->qctx);
	if (!JS_IsException(global)) {
		JS_SetPrototype(ctx->qctx, global, JS_NULL);
		JS_FreeValue(ctx->qctx, global);
	}
}

void duk_destroy_heap(duk_context *ctx)
{
	if (ctx == NULL) {
		return;
	}

	duk_free_child_contexts(ctx);
	duk_free_context_values(ctx);
	duk_drain_finalizers(ctx);
	duk_detach_qjs_global(ctx);
	JS_FreeValue(ctx->qctx, ctx->global_object);
	ctx->global_object = JS_UNDEFINED;
	duk_collect_and_drain(ctx);
	duk_free_child_context_structs(ctx);
	JS_FreeContext(ctx->qctx);
	JS_FreeRuntime(ctx->rt);
	free(ctx);
}

void duk_gc(duk_context *ctx, duk_uint_t flags)
{
	(void)flags;
	duk_collect_and_drain(ctx);
}

void duk_get_memory_functions(duk_context *ctx, duk_memory_functions *out_funcs)
{
	if (out_funcs != NULL) {
		*out_funcs = ctx->memfuncs;
	}
}

duk_idx_t duk_get_top(duk_context *ctx)
{
	return ctx->top;
}

void duk_set_top(duk_context *ctx, duk_idx_t top)
{
	duk_idx_t i;

	duk_clear_strings(ctx);
	if (top < 0) {
		top = ctx->top + top;
	}
	if (top < 0) {
		top = 0;
	}

	if (top < ctx->top) {
		for (i = top; i < ctx->top; i++) {
			JS_FreeValue(ctx->qctx, ctx->stack[i]);
		}
		ctx->top = top;
		return;
	}

	if (!duk_reserve_stack(ctx, top)) {
		return;
	}
	while (ctx->top < top) {
		ctx->stack[ctx->top++] = JS_UNDEFINED;
	}
}

duk_idx_t duk_normalize_index(duk_context *ctx, duk_idx_t idx)
{
	return duk_resolve_index(ctx, idx);
}

void duk_pop(duk_context *ctx)
{
	duk_pop_n(ctx, 1);
}

void duk_pop_2(duk_context *ctx)
{
	duk_pop_n(ctx, 2);
}

void duk_pop_3(duk_context *ctx)
{
	duk_pop_n(ctx, 3);
}

void duk_pop_n(duk_context *ctx, duk_idx_t count)
{
	if (count <= 0) {
		return;
	}
	if (count > ctx->top) {
		count = ctx->top;
	}
	duk_set_top(ctx, ctx->top - count);
}

void duk_dup(duk_context *ctx, duk_idx_t idx)
{
	idx = duk_resolve_index(ctx, idx);
	if (idx < 0) {
		duk_push_undefined(ctx);
		return;
	}
	duk_push_owned(ctx, JS_DupValue(ctx->qctx, ctx->stack[idx]));
}

void duk_dup_top(duk_context *ctx)
{
	duk_dup(ctx, -1);
}

void duk_insert(duk_context *ctx, duk_idx_t idx)
{
	JSValue val;

	idx = duk_resolve_index(ctx, idx);
	if (idx < 0 || ctx->top == 0 || idx == ctx->top - 1) {
		return;
	}

	val = ctx->stack[ctx->top - 1];
	memmove(&ctx->stack[idx + 1], &ctx->stack[idx],
			sizeof(*ctx->stack) * (ctx->top - idx - 1));
	ctx->stack[idx] = val;
}

void duk_remove(duk_context *ctx, duk_idx_t idx)
{
	idx = duk_resolve_index(ctx, idx);
	if (idx < 0) {
		return;
	}
	duk_free_range(ctx, idx, 1);
}

void duk_replace(duk_context *ctx, duk_idx_t idx)
{
	JSValue val;

	idx = duk_resolve_index(ctx, idx);
	if (idx < 0 || ctx->top == 0) {
		return;
	}
	val = ctx->stack[ctx->top - 1];
	ctx->top--;
	JS_FreeValue(ctx->qctx, ctx->stack[idx]);
	ctx->stack[idx] = val;
}

void duk_push_undefined(duk_context *ctx)
{
	duk_push_owned(ctx, JS_UNDEFINED);
}

void duk_push_null(duk_context *ctx)
{
	duk_push_owned(ctx, JS_NULL);
}

void duk_push_boolean(duk_context *ctx, duk_bool_t val)
{
	duk_push_owned(ctx, JS_NewBool(ctx->qctx, val));
}

void duk_push_int(duk_context *ctx, duk_int_t val)
{
	duk_push_owned(ctx, JS_NewInt32(ctx->qctx, val));
}

void duk_push_uint(duk_context *ctx, duk_uint_t val)
{
	duk_push_owned(ctx, JS_NewUint32(ctx->qctx, val));
}

void duk_push_number(duk_context *ctx, duk_double_t val)
{
	duk_push_owned(ctx, JS_NewFloat64(ctx->qctx, val));
}

void duk_push_string(duk_context *ctx, const char *str)
{
	duk_push_owned(ctx, str != NULL ?
			JS_NewString(ctx->qctx, str) : JS_NULL);
}

void duk_push_lstring(duk_context *ctx, const char *str, duk_size_t len)
{
	if (str == NULL) {
		duk_push_null(ctx);
		return;
	}
	duk_push_owned(ctx, JS_NewStringLen(ctx->qctx, str, len));
}

void duk_push_pointer(duk_context *ctx, void *ptr)
{
	struct duk_pointer_ref *pref;
	JSValue obj;

	pref = calloc(1, sizeof(*pref));
	if (pref == NULL) {
		duk_push_undefined(ctx);
		return;
	}
	pref->ptr = ptr;
	obj = JS_NewObjectClass(ctx->qctx, duk_pointer_class_id);
	if (JS_IsException(obj)) {
		free(pref);
		duk_push_owned(ctx, JS_GetException(ctx->qctx));
		return;
	}
	JS_SetOpaque(obj, pref);
	duk_push_owned(ctx, obj);
}

duk_idx_t duk_push_object(duk_context *ctx)
{
	struct duk_object_meta *meta;
	JSValue obj;

	meta = calloc(1, sizeof(*meta));
	if (meta == NULL) {
		duk_push_undefined(ctx);
		return ctx->top - 1;
	}
	meta->ctx = ctx;
	obj = JS_NewObjectClass(ctx->qctx, duk_object_class_id);
	if (JS_IsException(obj)) {
		free(meta);
		duk_push_owned(ctx, JS_GetException(ctx->qctx));
		return ctx->top - 1;
	}
	JS_SetOpaque(obj, meta);
	duk_push_owned(ctx, obj);
	return ctx->top - 1;
}

duk_idx_t duk_push_array(duk_context *ctx)
{
	duk_push_owned(ctx, JS_NewArray(ctx->qctx));
	return ctx->top - 1;
}

static void duk_free_array_buffer(JSRuntime *rt, void *opaque, void *ptr)
{
	(void)rt;
	(void)opaque;
	free(ptr);
}

void *duk_push_buffer(duk_context *ctx, duk_size_t size, duk_bool_t dynamic)
{
	void *buf;

	(void)dynamic;
	buf = calloc(1, size == 0 ? 1 : size);
	if (buf == NULL) {
		duk_push_undefined(ctx);
		return NULL;
	}

	duk_push_owned(ctx, JS_NewArrayBuffer(ctx->qctx, buf, size,
			duk_free_array_buffer, NULL, false));
	return buf;
}

void duk_push_buffer_object(duk_context *ctx, duk_idx_t idx,
		duk_size_t byte_offset, duk_size_t byte_length,
		duk_uint_t flags)
{
	JSValue abuf = duk_value_at(ctx, idx);
	JSValue args[3];
	JSValue view;

	(void)flags;

	args[0] = JS_DupValue(ctx->qctx, abuf);
	args[1] = JS_NewUint32(ctx->qctx, byte_offset);
	args[2] = JS_NewUint32(ctx->qctx, byte_length);
	view = JS_NewTypedArray(ctx->qctx, 3, args, JS_TYPED_ARRAY_UINT8C);
	JS_FreeValue(ctx->qctx, args[0]);
	JS_FreeValue(ctx->qctx, args[1]);
	JS_FreeValue(ctx->qctx, args[2]);
	duk_push_owned(ctx, view);
}

duk_idx_t duk_push_c_function(duk_context *ctx, duk_c_function func,
		duk_idx_t nargs)
{
	struct duk_cfunc_ref *ref;
	JSValue ref_obj;
	JSValue obj;
	JSValue data[1];
	int qjs_nargs = nargs < 0 ? 0 : nargs;

	ref = calloc(1, sizeof(*ref));
	if (ref == NULL) {
		duk_push_undefined(ctx);
		return ctx->top - 1;
	}
	ref->func = func;
	ref->nargs = nargs;

	ref_obj = JS_NewObjectClass(ctx->qctx, duk_cfunc_class_id);
	if (JS_IsException(ref_obj)) {
		free(ref);
		duk_push_owned(ctx, JS_GetException(ctx->qctx));
		return ctx->top - 1;
	}
	JS_SetOpaque(ref_obj, ref);

	data[0] = ref_obj;
	obj = JS_NewCFunctionData(ctx->qctx, duk_cfunc_data_call,
			qjs_nargs, 0, 1, data);
	if (JS_IsException(obj)) {
		JS_FreeValue(ctx->qctx, ref_obj);
		duk_push_owned(ctx, JS_GetException(ctx->qctx));
		return ctx->top - 1;
	}

	JS_SetConstructorBit(ctx->qctx, obj, true);
	JS_DefinePropertyValueStr(ctx->qctx, obj, duk_cfunc_ref_prop,
			JS_DupValue(ctx->qctx, ref_obj),
			JS_PROP_CONFIGURABLE);
	JS_FreeValue(ctx->qctx, ref_obj);
	duk_push_owned(ctx, obj);
	return ctx->top - 1;
}

void duk_push_global_object(duk_context *ctx)
{
	duk_push_owned(ctx, duk_get_effective_global(ctx));
}

void duk_push_this(duk_context *ctx)
{
	duk_push_owned(ctx, JS_DupValue(ctx->qctx, ctx->current_this));
}

void duk_push_thread(duk_context *ctx)
{
	duk_context *child;
	JSValue obj;

	child = calloc(1, sizeof(*child));
	if (child == NULL) {
		duk_push_undefined(ctx);
		return;
	}

	child->rt = ctx->rt;
	child->qctx = ctx->qctx;
	child->memfuncs = ctx->memfuncs;
	child->global_object = JS_UNDEFINED;
	child->current_this = JS_UNDEFINED;
	child->parent = ctx;
	child->next_child = ctx->children;
	ctx->children = child;

	obj = JS_NewObjectClass(ctx->qctx, duk_thread_class_id);
	if (JS_IsException(obj)) {
		duk_push_owned(ctx, JS_GetException(ctx->qctx));
		return;
	}
	JS_SetOpaque(obj, child);
	duk_push_owned(ctx, obj);
}

duk_idx_t duk_push_error_object(duk_context *ctx, duk_errcode_t err_code,
		const char *fmt, ...)
{
	va_list ap;
	char *msg = NULL;
	int len;

	(void)err_code;

	va_start(ap, fmt);
	len = vsnprintf(NULL, 0, fmt, ap);
	va_end(ap);
	if (len >= 0) {
		msg = malloc((size_t)len + 1);
		if (msg != NULL) {
			va_start(ap, fmt);
			vsnprintf(msg, (size_t)len + 1, fmt, ap);
			va_end(ap);
		}
	}

	duk_push_owned(ctx, duk_make_error(ctx, msg != NULL ? msg : ""));
	free(msg);
	return ctx->top - 1;
}

void duk_push_sprintf(duk_context *ctx, const char *fmt, ...)
{
	va_list ap;
	char *msg = NULL;
	int len;

	va_start(ap, fmt);
	len = vsnprintf(NULL, 0, fmt, ap);
	va_end(ap);
	if (len < 0) {
		duk_push_string(ctx, "");
		return;
	}

	msg = malloc((size_t)len + 1);
	if (msg == NULL) {
		duk_push_string(ctx, "");
		return;
	}

	va_start(ap, fmt);
	vsnprintf(msg, (size_t)len + 1, fmt, ap);
	va_end(ap);
	duk_push_lstring(ctx, msg, (duk_size_t)len);
	free(msg);
}

void duk_push_context_dump(duk_context *ctx)
{
	char buf[64];

	snprintf(buf, sizeof(buf), "[qjs stack top=%d]", ctx->top);
	duk_push_string(ctx, buf);
}

duk_context *duk_require_context(duk_context *ctx, duk_idx_t idx)
{
	JSValue val = duk_value_at(ctx, idx);
	duk_context *child = JS_GetOpaque(val, duk_thread_class_id);

	return child;
}

duk_bool_t duk_get_prop(duk_context *ctx, duk_idx_t obj_idx)
{
	duk_idx_t obj_pos = duk_resolve_index(ctx, obj_idx);
	JSAtom atom;
	JSValue val;
	bool exists;

	if (obj_pos < 0 || ctx->top < 1) {
		duk_push_undefined(ctx);
		return false;
	}

	atom = duk_atom_from_value(ctx, ctx->stack[ctx->top - 1]);
	JS_FreeValue(ctx->qctx, ctx->stack[ctx->top - 1]);
	ctx->top--;
	if (atom == JS_ATOM_NULL) {
		duk_push_undefined(ctx);
		return false;
	}

	val = JS_GetProperty(ctx->qctx, ctx->stack[obj_pos], atom);
	JS_FreeAtom(ctx->qctx, atom);
	exists = !JS_IsUndefined(val);
	duk_push_owned(ctx, val);
	return exists;
}

duk_bool_t duk_get_prop_string(duk_context *ctx, duk_idx_t obj_idx,
		const char *key)
{
	duk_idx_t obj_pos = duk_resolve_index(ctx, obj_idx);
	JSValue obj = obj_pos >= 0 ? ctx->stack[obj_pos] : JS_UNDEFINED;
	JSValue val;
	bool exists;

	if (ctx->finalizer_private_ptr != NULL && obj_pos == 0 &&
			strcmp(key, duk_private_magic_prop) == 0) {
		duk_push_pointer(ctx, ctx->finalizer_private_ptr);
		return true;
	}

	val = JS_GetPropertyStr(ctx->qctx, obj, key);
	if (JS_IsUndefined(val) && duk_is_effective_global_value(ctx, obj)) {
		JSValue global = JS_GetGlobalObject(ctx->qctx);

		JS_FreeValue(ctx->qctx, val);
		val = JS_GetPropertyStr(ctx->qctx, global, key);
		JS_FreeValue(ctx->qctx, global);
	}
	exists = !JS_IsUndefined(val);

	duk_push_owned(ctx, val);
	return exists;
}

duk_bool_t duk_get_prop_index(duk_context *ctx, duk_idx_t obj_idx,
		duk_uarridx_t idx)
{
	JSValue obj = duk_value_at(ctx, obj_idx);
	JSValue val = JS_GetPropertyUint32(ctx->qctx, obj, idx);
	bool exists = !JS_IsUndefined(val);

	duk_push_owned(ctx, val);
	return exists;
}

duk_bool_t duk_get_global_string(duk_context *ctx, const char *key)
{
	JSValue global = duk_get_effective_global(ctx);
	JSValue val = JS_GetPropertyStr(ctx->qctx, global, key);
	bool exists = !JS_IsUndefined(val);

	JS_FreeValue(ctx->qctx, global);
	duk_push_owned(ctx, val);
	return exists;
}

duk_bool_t duk_put_prop(duk_context *ctx, duk_idx_t obj_idx)
{
	duk_idx_t obj_pos = duk_resolve_index(ctx, obj_idx);
	JSAtom atom;
	JSValue key;
	JSValue val;
	int ret;

	if (obj_pos < 0 || ctx->top < 2) {
		return false;
	}

	key = ctx->stack[ctx->top - 2];
	val = ctx->stack[ctx->top - 1];
	atom = duk_atom_from_value(ctx, key);
	JS_FreeValue(ctx->qctx, key);
	ctx->top -= 2;
	if (atom == JS_ATOM_NULL) {
		JS_FreeValue(ctx->qctx, val);
		return false;
	}

	ret = JS_SetProperty(ctx->qctx, ctx->stack[obj_pos], atom, val);
	JS_FreeAtom(ctx->qctx, atom);
	return ret >= 0;
}

duk_bool_t duk_put_prop_string(duk_context *ctx, duk_idx_t obj_idx,
		const char *key)
{
	duk_idx_t obj_pos = duk_resolve_index(ctx, obj_idx);
	struct duk_object_meta *meta;
	struct duk_pointer_ref *pref;
	JSValue val;
	int ret;

	if (obj_pos < 0 || ctx->top < 1) {
		return false;
	}

	val = ctx->stack[ctx->top - 1];
	ctx->top--;
	meta = JS_GetOpaque(ctx->stack[obj_pos], duk_object_class_id);
	pref = JS_GetOpaque(val, duk_pointer_class_id);
	if (meta != NULL && pref != NULL &&
			strcmp(key, duk_private_magic_prop) == 0) {
		meta->private_ptr = pref->ptr;
	}
	ret = JS_SetPropertyStr(ctx->qctx, ctx->stack[obj_pos], key, val);
	return ret >= 0;
}

duk_bool_t duk_put_prop_index(duk_context *ctx, duk_idx_t obj_idx,
		duk_uarridx_t idx)
{
	duk_idx_t obj_pos = duk_resolve_index(ctx, obj_idx);
	JSValue val;
	int ret;

	if (obj_pos < 0 || ctx->top < 1) {
		return false;
	}

	val = ctx->stack[ctx->top - 1];
	ctx->top--;
	ret = JS_SetPropertyUint32(ctx->qctx, ctx->stack[obj_pos], idx, val);
	return ret >= 0;
}

duk_bool_t duk_put_global_string(duk_context *ctx, const char *key)
{
	JSValue global;
	JSValue val;
	int ret;

	if (ctx->top < 1) {
		return false;
	}

	global = duk_get_effective_global(ctx);
	val = ctx->stack[ctx->top - 1];
	ctx->top--;
	ret = JS_SetPropertyStr(ctx->qctx, global, key, val);
	JS_FreeValue(ctx->qctx, global);
	return ret >= 0;
}

duk_bool_t duk_del_prop(duk_context *ctx, duk_idx_t obj_idx)
{
	duk_idx_t obj_pos = duk_resolve_index(ctx, obj_idx);
	JSAtom atom;
	JSValue key;
	int ret;

	if (obj_pos < 0 || ctx->top < 1) {
		return false;
	}

	key = ctx->stack[ctx->top - 1];
	atom = duk_atom_from_value(ctx, key);
	JS_FreeValue(ctx->qctx, key);
	ctx->top--;
	if (atom == JS_ATOM_NULL) {
		return false;
	}

	ret = JS_DeleteProperty(ctx->qctx, ctx->stack[obj_pos], atom, 0);
	JS_FreeAtom(ctx->qctx, atom);
	return ret >= 0;
}

duk_bool_t duk_del_prop_string(duk_context *ctx, duk_idx_t obj_idx,
		const char *key)
{
	JSAtom atom;
	int ret;
	JSValue obj = duk_value_at(ctx, obj_idx);

	atom = JS_NewAtom(ctx->qctx, key);
	if (atom == JS_ATOM_NULL) {
		return false;
	}
	ret = JS_DeleteProperty(ctx->qctx, obj, atom, 0);
	JS_FreeAtom(ctx->qctx, atom);
	return ret >= 0;
}

duk_bool_t duk_del_prop_index(duk_context *ctx, duk_idx_t obj_idx,
		duk_uarridx_t idx)
{
	JSAtom atom;
	int ret;
	JSValue obj = duk_value_at(ctx, obj_idx);

	atom = JS_NewAtomUInt32(ctx->qctx, idx);
	if (atom == JS_ATOM_NULL) {
		return false;
	}
	ret = JS_DeleteProperty(ctx->qctx, obj, atom, 0);
	JS_FreeAtom(ctx->qctx, atom);
	return ret >= 0;
}

duk_bool_t duk_has_prop(duk_context *ctx, duk_idx_t obj_idx)
{
	duk_idx_t obj_pos = duk_resolve_index(ctx, obj_idx);
	JSAtom atom;
	JSValue key;
	int ret;

	if (obj_pos < 0 || ctx->top < 1) {
		return false;
	}

	key = ctx->stack[ctx->top - 1];
	atom = duk_atom_from_value(ctx, key);
	JS_FreeValue(ctx->qctx, key);
	ctx->top--;
	if (atom == JS_ATOM_NULL) {
		return false;
	}
	ret = JS_HasProperty(ctx->qctx, ctx->stack[obj_pos], atom);
	JS_FreeAtom(ctx->qctx, atom);
	return ret > 0;
}

static int duk_defprop_flags_to_qjs(duk_uint_t flags)
{
	int qflags = 0;

	if (flags & DUK_DEFPROP_HAVE_VALUE) {
		qflags |= JS_PROP_HAS_VALUE;
	}
	if (flags & DUK_DEFPROP_HAVE_WRITABLE) {
		qflags |= JS_PROP_HAS_WRITABLE;
		if (flags & DUK_DEFPROP_WRITABLE) {
			qflags |= JS_PROP_WRITABLE;
		}
	}
	if (flags & DUK_DEFPROP_HAVE_ENUMERABLE) {
		qflags |= JS_PROP_HAS_ENUMERABLE;
		if (flags & DUK_DEFPROP_ENUMERABLE) {
			qflags |= JS_PROP_ENUMERABLE;
		}
	}
	if (flags & DUK_DEFPROP_HAVE_CONFIGURABLE) {
		qflags |= JS_PROP_HAS_CONFIGURABLE;
		if (flags & DUK_DEFPROP_CONFIGURABLE) {
			qflags |= JS_PROP_CONFIGURABLE;
		}
	}
	if (flags & DUK_DEFPROP_HAVE_GETTER) {
		qflags |= JS_PROP_HAS_GET;
	}
	if (flags & DUK_DEFPROP_HAVE_SETTER) {
		qflags |= JS_PROP_HAS_SET;
	}

	return qflags;
}

void duk_def_prop(duk_context *ctx, duk_idx_t obj_idx, duk_uint_t flags)
{
	duk_idx_t obj_pos = duk_resolve_index(ctx, obj_idx);
	duk_idx_t nvals = 0;
	duk_idx_t key_pos;
	JSAtom atom;
	JSValue value = JS_UNDEFINED;
	JSValue getter = JS_UNDEFINED;
	JSValue setter = JS_UNDEFINED;

	if (obj_pos < 0) {
		return;
	}

	if (flags & DUK_DEFPROP_HAVE_VALUE) {
		nvals++;
	}
	if (flags & DUK_DEFPROP_HAVE_GETTER) {
		nvals++;
	}
	if (flags & DUK_DEFPROP_HAVE_SETTER) {
		nvals++;
	}
	if (ctx->top < nvals + 1) {
		return;
	}

	key_pos = ctx->top - nvals - 1;
	atom = duk_atom_from_value(ctx, ctx->stack[key_pos]);
	if (atom == JS_ATOM_NULL) {
		duk_pop_n(ctx, nvals + 1);
		return;
	}

	if (flags & DUK_DEFPROP_HAVE_VALUE) {
		value = JS_DupValue(ctx->qctx, ctx->stack[key_pos + 1]);
	} else if (flags & DUK_DEFPROP_HAVE_GETTER) {
		getter = JS_DupValue(ctx->qctx, ctx->stack[key_pos + 1]);
		if (flags & DUK_DEFPROP_HAVE_SETTER) {
			setter = JS_DupValue(ctx->qctx, ctx->stack[key_pos + 2]);
		}
	}

	JS_DefineProperty(ctx->qctx, ctx->stack[obj_pos], atom, value,
			getter, setter, duk_defprop_flags_to_qjs(flags));
	JS_FreeAtom(ctx->qctx, atom);
	JS_FreeValue(ctx->qctx, value);
	JS_FreeValue(ctx->qctx, getter);
	JS_FreeValue(ctx->qctx, setter);
	duk_pop_n(ctx, nvals + 1);
}

void duk_set_prototype(duk_context *ctx, duk_idx_t idx)
{
	duk_idx_t obj_pos = duk_resolve_index(ctx, idx);
	JSValue proto;
	struct duk_object_meta *obj_meta;
	struct duk_object_meta *proto_meta;

	if (obj_pos < 0 || ctx->top < 1) {
		return;
	}

	proto = ctx->stack[ctx->top - 1];
	obj_meta = JS_GetOpaque(ctx->stack[obj_pos], duk_object_class_id);
	proto_meta = JS_GetOpaque(proto, duk_object_class_id);
	if (obj_meta != NULL && proto_meta != NULL &&
			obj_meta->finalizer == NULL) {
		obj_meta->finalizer = proto_meta->finalizer;
	}
	JS_SetPrototype(ctx->qctx, ctx->stack[obj_pos], proto);
	JS_FreeValue(ctx->qctx, proto);
	ctx->top--;
}

void duk_get_prototype(duk_context *ctx, duk_idx_t idx)
{
	JSValue proto = JS_GetPrototype(ctx->qctx, duk_value_at(ctx, idx));

	if (JS_IsNull(proto)) {
		JS_FreeValue(ctx->qctx, proto);
		proto = JS_UNDEFINED;
	}
	duk_push_owned(ctx, proto);
}

void duk_set_finalizer(duk_context *ctx, duk_idx_t idx)
{
	duk_idx_t obj_pos = duk_resolve_index(ctx, idx);
	struct duk_object_meta *meta;
	struct duk_cfunc_ref *ref = NULL;

	if (obj_pos < 0 || ctx->top < 1) {
		return;
	}

	meta = JS_GetOpaque(ctx->stack[obj_pos], duk_object_class_id);
	ref = duk_get_cfunc_ref(ctx, ctx->stack[ctx->top - 1]);
	if (meta != NULL) {
		meta->finalizer = ref != NULL ? ref->func : NULL;
	}
	duk_pop(ctx);
}

void duk_set_global_object(duk_context *ctx)
{
	JSValue global;
	JSValue win;

	if (ctx->top < 1) {
		return;
	}

	win = ctx->stack[ctx->top - 1];
	JS_FreeValue(ctx->qctx, ctx->global_object);
	ctx->global_object = JS_DupValue(ctx->qctx, win);
	global = JS_GetGlobalObject(ctx->qctx);
	JS_SetPrototype(ctx->qctx, global, win);
	JS_FreeValue(ctx->qctx, global);
	JS_SetContextOpaque(ctx->qctx, ctx);
	JS_FreeValue(ctx->qctx, win);
	ctx->top--;
}

static void duk_frame_enter(duk_context *ctx, JSValueConst this_val,
		int argc, JSValueConst *argv, JSValue **old_stack,
		duk_idx_t *old_top, duk_idx_t *old_cap, JSValue *old_this)
{
	int i;

	*old_stack = ctx->stack;
	*old_top = ctx->top;
	*old_cap = ctx->cap;
	*old_this = ctx->current_this;

	ctx->stack = NULL;
	ctx->top = 0;
	ctx->cap = 0;
	ctx->current_this = JS_DupValue(ctx->qctx, this_val);
	duk_clear_strings(ctx);
	free(ctx->pending_error);
	ctx->pending_error = NULL;

	duk_reserve_stack(ctx, argc);
	for (i = 0; i < argc; i++) {
		ctx->stack[ctx->top++] = JS_DupValue(ctx->qctx, argv[i]);
	}
}

static void duk_frame_leave(duk_context *ctx, JSValue *old_stack,
		duk_idx_t old_top, duk_idx_t old_cap, JSValue old_this)
{
	duk_free_context_values(ctx);
	ctx->stack = old_stack;
	ctx->top = old_top;
	ctx->cap = old_cap;
	ctx->current_this = old_this;
}

static JSValue duk_call_c_function_value(duk_context *ctx,
		duk_c_function func, JSValueConst this_val, int argc,
		JSValueConst *argv)
{
	JSValue *old_stack;
	JSValue old_this;
	duk_idx_t old_top;
	duk_idx_t old_cap;
	duk_ret_t ret;
	JSValue result = JS_UNDEFINED;

	duk_frame_enter(ctx, this_val, argc, argv, &old_stack, &old_top,
			&old_cap, &old_this);

	ret = func(ctx);
	if (ret < 0) {
		result = duk_throw_pending(ctx);
	} else if (ret > 0 && ctx->top >= ret) {
		result = ctx->stack[ctx->top - ret];
		ctx->stack[ctx->top - ret] = JS_UNDEFINED;
	} else {
		result = JS_UNDEFINED;
	}

	duk_frame_leave(ctx, old_stack, old_top, old_cap, old_this);
	duk_drain_finalizers(ctx);
	return result;
}

static JSValue duk_cfunc_call(JSContext *qctx, JSValueConst func_obj,
		JSValueConst this_val, int argc, JSValueConst *argv,
		int flags)
{
	struct duk_cfunc_ref *ref = JS_GetOpaque(func_obj, duk_cfunc_class_id);
	duk_context *ctx = JS_GetContextOpaque(qctx);

	(void)flags;

	if (ctx == NULL || ref == NULL || ref->func == NULL) {
		return JS_ThrowTypeError(qctx, "invalid Slate C function");
	}

	return duk_call_c_function_value(ctx, ref->func, this_val, argc, argv);
}

static JSValue duk_cfunc_data_call(JSContext *qctx, JSValueConst this_val,
		int argc, JSValueConst *argv, int magic, JSValue *func_data)
{
	struct duk_cfunc_ref *ref = JS_GetOpaque(func_data[0],
			duk_cfunc_class_id);
	duk_context *ctx = JS_GetContextOpaque(qctx);

	(void)magic;

	if (ctx == NULL || ref == NULL || ref->func == NULL) {
		return JS_ThrowTypeError(qctx, "invalid Slate C function");
	}

	return duk_call_c_function_value(ctx, ref->func, this_val, argc, argv);
}

static struct duk_cfunc_ref *duk_get_cfunc_ref(duk_context *ctx,
		JSValueConst val)
{
	struct duk_cfunc_ref *ref;
	JSValue ref_obj;

	ref = JS_GetOpaque(val, duk_cfunc_class_id);
	if (ref != NULL) {
		return ref;
	}

	ref_obj = JS_GetPropertyStr(ctx->qctx, val, duk_cfunc_ref_prop);
	if (JS_IsException(ref_obj)) {
		JS_FreeValue(ctx->qctx, JS_GetException(ctx->qctx));
		return NULL;
	}
	ref = JS_GetOpaque(ref_obj, duk_cfunc_class_id);
	JS_FreeValue(ctx->qctx, ref_obj);
	return ref;
}

static void duk_replace_call_values(duk_context *ctx, duk_idx_t start,
		duk_idx_t count, JSValue result)
{
	duk_free_range(ctx, start, count);
	duk_push_owned(ctx, result);
}

static duk_int_t duk_call_js(duk_context *ctx, duk_idx_t func_pos,
		JSValueConst this_val, duk_idx_t nargs, duk_idx_t consume_count)
{
	JSValue func = ctx->stack[func_pos];
	JSValue result;
	void *old_opaque;

	old_opaque = JS_GetContextOpaque(ctx->qctx);
	duk_sync_qjs_global(ctx);

	if (JS_VALUE_GET_TAG(func) == JS_TAG_FUNCTION_BYTECODE) {
		result = JS_EvalFunction(ctx->qctx, JS_DupValue(ctx->qctx, func));
	} else {
		result = JS_Call(ctx->qctx, func, this_val, nargs,
				(JSValueConst *)&ctx->stack[func_pos + consume_count - nargs]);
	}

	if (JS_IsException(result)) {
		result = JS_GetException(ctx->qctx);
		JS_SetContextOpaque(ctx->qctx, old_opaque);
		duk_replace_call_values(ctx, func_pos, consume_count, result);
		duk_drain_finalizers(ctx);
		return DUK_EXEC_ERROR;
	}

	JS_SetContextOpaque(ctx->qctx, old_opaque);
	duk_replace_call_values(ctx, func_pos, consume_count, result);
	duk_drain_finalizers(ctx);
	return DUK_EXEC_SUCCESS;
}

void duk_call(duk_context *ctx, duk_idx_t nargs)
{
	duk_idx_t func_pos = ctx->top - nargs - 1;

	if (func_pos < 0) {
		duk_push_undefined(ctx);
		return;
	}
	(void)duk_call_js(ctx, func_pos, JS_UNDEFINED, nargs, nargs + 1);
}

duk_int_t duk_pcall(duk_context *ctx, duk_idx_t nargs)
{
	duk_idx_t func_pos = ctx->top - nargs - 1;

	if (func_pos < 0) {
		duk_push_undefined(ctx);
		return DUK_EXEC_ERROR;
	}
	return duk_call_js(ctx, func_pos, JS_UNDEFINED, nargs, nargs + 1);
}

duk_int_t duk_pcall_method(duk_context *ctx, duk_idx_t nargs)
{
	duk_idx_t func_pos = ctx->top - nargs - 2;
	JSValue this_val;

	if (func_pos < 0) {
		duk_push_undefined(ctx);
		return DUK_EXEC_ERROR;
	}

	this_val = ctx->stack[func_pos + 1];
	return duk_call_js(ctx, func_pos, this_val, nargs, nargs + 2);
}

duk_int_t duk_safe_call(duk_context *ctx, duk_safe_call_function func,
		void *udata, duk_idx_t nargs, duk_idx_t nrets)
{
	duk_idx_t base = ctx->top - nargs;
	JSValue *old_stack;
	JSValue old_this;
	JSValue *results = NULL;
	duk_idx_t old_top;
	duk_idx_t old_cap;
	duk_idx_t i;
	duk_ret_t ret;

	if (base < 0) {
		return DUK_EXEC_ERROR;
	}

	results = calloc(nrets > 0 ? nrets : 1, sizeof(*results));
	if (results == NULL) {
		return DUK_EXEC_ERROR;
	}
	for (i = 0; i < nrets; i++) {
		results[i] = JS_UNDEFINED;
	}

	old_stack = ctx->stack;
	old_top = base;
	old_cap = ctx->cap;
	old_this = ctx->current_this;

	ctx->stack = NULL;
	ctx->top = 0;
	ctx->cap = 0;
	ctx->current_this = JS_UNDEFINED;
	duk_reserve_stack(ctx, nargs);
	for (i = 0; i < nargs; i++) {
		ctx->stack[ctx->top++] = old_stack[base + i];
		old_stack[base + i] = JS_UNDEFINED;
	}

	ret = func(ctx, udata);
	if (ret < 0) {
		JSValue err = duk_make_error(ctx, ctx->pending_error);
		duk_free_context_values(ctx);
		ctx->stack = old_stack;
		ctx->top = old_top;
		ctx->cap = old_cap;
		ctx->current_this = old_this;
		duk_push_owned(ctx, err);
		free(results);
		duk_drain_finalizers(ctx);
		return DUK_EXEC_ERROR;
	}

	for (i = 0; i < nrets; i++) {
		if (ctx->top >= nrets - i) {
			results[i] = ctx->stack[ctx->top - nrets + i];
			ctx->stack[ctx->top - nrets + i] = JS_UNDEFINED;
		}
	}
	duk_free_context_values(ctx);
	ctx->stack = old_stack;
	ctx->top = old_top;
	ctx->cap = old_cap;
	ctx->current_this = old_this;

	for (i = 0; i < nrets; i++) {
		duk_push_owned(ctx, results[i]);
	}
	free(results);
	duk_drain_finalizers(ctx);
	return DUK_EXEC_SUCCESS;
}

static char *duk_copy_nul_terminated(const char *src, duk_size_t len)
{
	char *copy = malloc(len + 1);

	if (copy == NULL) {
		return NULL;
	}
	memcpy(copy, src, len);
	copy[len] = '\0';
	return copy;
}

duk_int_t duk_pcompile_lstring_filename(duk_context *ctx, duk_uint_t flags,
		const char *src, duk_size_t len)
{
	const char *filename;
	char *source;
	JSValue compiled;
	void *old_opaque;

	if (ctx->top < 1 || src == NULL) {
		duk_push_string(ctx, "missing compile input");
		return DUK_EXEC_ERROR;
	}

	while (len > 0 && src[len - 1] == '\0') {
		len--;
	}

	filename = duk_safe_to_string(ctx, -1);
	source = duk_copy_nul_terminated(src, len);
	if (source == NULL) {
		duk_pop(ctx);
		duk_push_string(ctx, "out of memory compiling JavaScript");
		return DUK_EXEC_ERROR;
	}

	old_opaque = JS_GetContextOpaque(ctx->qctx);
	duk_sync_qjs_global(ctx);

	if (flags & DUK_COMPILE_FUNCTION) {
		size_t wrap_len = len + 3;
		char *wrapped = malloc(wrap_len);
		if (wrapped == NULL) {
			JS_SetContextOpaque(ctx->qctx, old_opaque);
			free(source);
			duk_pop(ctx);
			duk_push_string(ctx, "out of memory compiling JavaScript");
			return DUK_EXEC_ERROR;
		}
		wrapped[0] = '(';
		memcpy(wrapped + 1, src, len);
		wrapped[len + 1] = ')';
		wrapped[len + 2] = '\0';
		compiled = JS_Eval(ctx->qctx, wrapped, wrap_len - 1, filename,
				JS_EVAL_TYPE_GLOBAL);
		free(wrapped);
	} else {
		compiled = JS_Eval(ctx->qctx, source, len, filename,
				JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_COMPILE_ONLY);
	}
	JS_SetContextOpaque(ctx->qctx, old_opaque);
	free(source);
	duk_pop(ctx);

	if (JS_IsException(compiled)) {
		duk_push_owned(ctx, JS_GetException(ctx->qctx));
		return DUK_EXEC_ERROR;
	}

	duk_push_owned(ctx, compiled);
	return DUK_EXEC_SUCCESS;
}

duk_int_t duk_pcompile(duk_context *ctx, duk_uint_t flags)
{
	const char *source;
	char *source_copy;
	duk_size_t source_len = 0;
	duk_idx_t base;
	JSValue result;
	duk_int_t ret;

	if (ctx->top < 2) {
		duk_push_string(ctx, "missing compile input");
		return DUK_EXEC_ERROR;
	}

	base = ctx->top - 2;
	source = duk_safe_to_lstring(ctx, -2, &source_len);
	if (source == NULL) {
		duk_push_string(ctx, "missing compile source");
		return DUK_EXEC_ERROR;
	}
	source_copy = duk_copy_nul_terminated(source, source_len);
	if (source_copy == NULL) {
		duk_push_string(ctx, "out of memory compiling JavaScript");
		return DUK_EXEC_ERROR;
	}

	duk_dup(ctx, -1);
	ret = duk_pcompile_lstring_filename(ctx, flags, source_copy, source_len);
	free(source_copy);

	result = ctx->stack[ctx->top - 1];
	ctx->stack[ctx->top - 1] = JS_UNDEFINED;
	duk_free_range(ctx, base, ctx->top - base);
	ctx->top = base;
	duk_push_owned(ctx, result);
	return ret;
}

duk_int_t duk_get_type(duk_context *ctx, duk_idx_t idx)
{
	return duk_qjs_type_to_duk(duk_value_at(ctx, idx));
}

duk_bool_t duk_check_type(duk_context *ctx, duk_idx_t idx, duk_int_t type)
{
	return duk_get_type(ctx, idx) == type;
}

duk_bool_t duk_is_undefined(duk_context *ctx, duk_idx_t idx)
{
	return JS_IsUndefined(duk_value_at(ctx, idx));
}

duk_bool_t duk_is_null(duk_context *ctx, duk_idx_t idx)
{
	return JS_IsNull(duk_value_at(ctx, idx));
}

duk_bool_t duk_is_null_or_undefined(duk_context *ctx, duk_idx_t idx)
{
	JSValue val = duk_value_at(ctx, idx);
	return JS_IsNull(val) || JS_IsUndefined(val);
}

duk_bool_t duk_is_boolean(duk_context *ctx, duk_idx_t idx)
{
	return JS_IsBool(duk_value_at(ctx, idx));
}

duk_bool_t duk_is_number(duk_context *ctx, duk_idx_t idx)
{
	return JS_IsNumber(duk_value_at(ctx, idx));
}

duk_bool_t duk_is_string(duk_context *ctx, duk_idx_t idx)
{
	return JS_IsString(duk_value_at(ctx, idx));
}

duk_bool_t duk_get_boolean(duk_context *ctx, duk_idx_t idx)
{
	int ret = JS_ToBool(ctx->qctx, duk_value_at(ctx, idx));
	return ret > 0;
}

duk_int_t duk_get_int(duk_context *ctx, duk_idx_t idx)
{
	int32_t ret = 0;
	JS_ToInt32(ctx->qctx, &ret, duk_value_at(ctx, idx));
	return ret;
}

duk_uint_t duk_get_uint(duk_context *ctx, duk_idx_t idx)
{
	uint32_t ret = 0;
	JS_ToUint32(ctx->qctx, &ret, duk_value_at(ctx, idx));
	return ret;
}

void *duk_get_pointer(duk_context *ctx, duk_idx_t idx)
{
	struct duk_pointer_ref *pref;

	pref = JS_GetOpaque(duk_value_at(ctx, idx), duk_pointer_class_id);
	return pref != NULL ? pref->ptr : NULL;
}

const char *duk_get_string(duk_context *ctx, duk_idx_t idx)
{
	JSValue val = duk_value_at(ctx, idx);

	if (!JS_IsString(val)) {
		return NULL;
	}
	return duk_value_to_cached_string(ctx, val, NULL, false);
}

duk_size_t duk_get_length(duk_context *ctx, duk_idx_t idx)
{
	JSValue val = duk_value_at(ctx, idx);

	if (JS_IsString(val)) {
		duk_size_t len = 0;
		const char *str = duk_value_to_cached_string(ctx, val, &len, true);
		(void)str;
		return len;
	}

	JSValue length = JS_GetPropertyStr(ctx->qctx, val, "length");
	uint32_t ret = 0;
	JS_ToUint32(ctx->qctx, &ret, length);
	JS_FreeValue(ctx->qctx, length);
	return ret;
}

duk_bool_t duk_to_boolean(duk_context *ctx, duk_idx_t idx)
{
	JSValue *slot = duk_value_slot(ctx, idx);
	duk_bool_t ret;

	if (slot == NULL) {
		return false;
	}
	ret = JS_ToBool(ctx->qctx, *slot) > 0;
	JS_FreeValue(ctx->qctx, *slot);
	*slot = JS_NewBool(ctx->qctx, ret);
	return ret;
}

duk_int_t duk_to_int(duk_context *ctx, duk_idx_t idx)
{
	JSValue *slot = duk_value_slot(ctx, idx);
	int32_t ret = 0;

	if (slot == NULL) {
		return 0;
	}
	JS_ToInt32(ctx->qctx, &ret, *slot);
	JS_FreeValue(ctx->qctx, *slot);
	*slot = JS_NewInt32(ctx->qctx, ret);
	return ret;
}

duk_uint_t duk_to_uint(duk_context *ctx, duk_idx_t idx)
{
	JSValue *slot = duk_value_slot(ctx, idx);
	uint32_t ret = 0;

	if (slot == NULL) {
		return 0;
	}
	JS_ToUint32(ctx->qctx, &ret, *slot);
	JS_FreeValue(ctx->qctx, *slot);
	*slot = JS_NewUint32(ctx->qctx, ret);
	return ret;
}

uint32_t duk_to_uint32(duk_context *ctx, duk_idx_t idx)
{
	return duk_to_uint(ctx, idx);
}

const char *duk_to_string(duk_context *ctx, duk_idx_t idx)
{
	JSValue *slot = duk_value_slot(ctx, idx);
	JSValue str;

	if (slot == NULL) {
		return NULL;
	}
	str = JS_ToString(ctx->qctx, *slot);
	if (JS_IsException(str)) {
		JSValue exc = JS_GetException(ctx->qctx);
		JS_FreeValue(ctx->qctx, exc);
		return NULL;
	}
	JS_FreeValue(ctx->qctx, *slot);
	*slot = str;
	return duk_value_to_cached_string(ctx, *slot, NULL, true);
}

const char *duk_to_lstring(duk_context *ctx, duk_idx_t idx, duk_size_t *out_len)
{
	const char *ret = duk_to_string(ctx, idx);

	if (ret == NULL && out_len != NULL) {
		*out_len = 0;
		return NULL;
	}
	if (out_len != NULL) {
		*out_len = strlen(ret);
	}
	return ret;
}

duk_bool_t duk_require_boolean(duk_context *ctx, duk_idx_t idx)
{
	return duk_get_boolean(ctx, idx);
}

duk_int_t duk_require_int(duk_context *ctx, duk_idx_t idx)
{
	return duk_get_int(ctx, idx);
}

duk_double_t duk_require_number(duk_context *ctx, duk_idx_t idx)
{
	double ret = 0;
	JS_ToFloat64(ctx->qctx, &ret, duk_value_at(ctx, idx));
	return ret;
}

const char *duk_require_string(duk_context *ctx, duk_idx_t idx)
{
	return duk_safe_to_string(ctx, idx);
}

const char *duk_require_lstring(duk_context *ctx, duk_idx_t idx,
		duk_size_t *out_len)
{
	return duk_safe_to_lstring(ctx, idx, out_len);
}

duk_bool_t duk_opt_boolean(duk_context *ctx, duk_idx_t idx,
		duk_bool_t def_val)
{
	if (duk_is_undefined(ctx, idx)) {
		return def_val;
	}
	return duk_get_boolean(ctx, idx);
}

const char *duk_safe_to_string(duk_context *ctx, duk_idx_t idx)
{
	return duk_safe_to_lstring(ctx, idx, NULL);
}

const char *duk_safe_to_lstring(duk_context *ctx, duk_idx_t idx,
		duk_size_t *out_len)
{
	return duk_value_to_cached_string(ctx, duk_value_at(ctx, idx),
			out_len, true);
}

const char *duk_safe_to_stacktrace(duk_context *ctx, duk_idx_t idx)
{
	JSValue val = duk_value_at(ctx, idx);
	JSValue stack;
	const char *ret;

	if (JS_IsObject(val)) {
		stack = JS_GetPropertyStr(ctx->qctx, val, "stack");
		if (!JS_IsUndefined(stack)) {
			ret = duk_value_to_cached_string(ctx, stack, NULL, true);
			JS_FreeValue(ctx->qctx, stack);
			return ret;
		}
		JS_FreeValue(ctx->qctx, stack);
	}

	return duk_safe_to_string(ctx, idx);
}

duk_bool_t duk_strict_equals(duk_context *ctx, duk_idx_t idx1,
		duk_idx_t idx2)
{
	return JS_StrictEq(ctx->qctx, duk_value_at(ctx, idx1),
			duk_value_at(ctx, idx2));
}

void duk_concat(duk_context *ctx, duk_idx_t count)
{
	duk_idx_t start;
	duk_idx_t i;
	size_t total = 0;
	char *buf;
	size_t off = 0;

	if (count <= 0) {
		duk_push_string(ctx, "");
		return;
	}
	if (count > ctx->top) {
		count = ctx->top;
	}

	start = ctx->top - count;
	for (i = start; i < ctx->top; i++) {
		duk_size_t len = 0;
		(void)duk_value_to_cached_string(ctx, ctx->stack[i], &len, true);
		total += len;
	}

	buf = malloc(total + 1);
	if (buf == NULL) {
		duk_pop_n(ctx, count);
		duk_push_string(ctx, "");
		return;
	}

	for (i = start; i < ctx->top; i++) {
		duk_size_t len = 0;
		const char *part = duk_value_to_cached_string(ctx, ctx->stack[i],
				&len, true);
		memcpy(buf + off, part, len);
		off += len;
	}
	buf[off] = '\0';
	duk_pop_n(ctx, count);
	duk_push_lstring(ctx, buf, off);
	free(buf);
}

duk_ret_t duk_error(duk_context *ctx, duk_errcode_t err_code,
		const char *fmt, ...)
{
	va_list ap;

	(void)err_code;

	va_start(ap, fmt);
	duk_set_pending_errorv(ctx, fmt, ap);
	va_end(ap);
	return DUK_RET_TYPE_ERROR;
}

duk_ret_t duk_generic_error(duk_context *ctx, const char *fmt, ...)
{
	va_list ap;

	va_start(ap, fmt);
	duk_set_pending_errorv(ctx, fmt, ap);
	va_end(ap);
	return DUK_RET_TYPE_ERROR;
}

static void duk_object_finalizer(JSRuntime *rt, JSValue val)
{
	struct duk_object_meta *meta = JS_GetOpaque(val, duk_object_class_id);

	(void)rt;
	/*
	 * QuickJS calls this after object properties are released. The generated
	 * destructor still expects Duktape-style private lookup, so queue it and
	 * provide the cached native private pointer from a safe shim frame.
	 */
	duk_queue_finalizer(meta);
}

static void duk_cfunc_finalizer(JSRuntime *rt, JSValue val)
{
	struct duk_cfunc_ref *ref = JS_GetOpaque(val, duk_cfunc_class_id);

	(void)rt;
	free(ref);
}

static void duk_pointer_finalizer(JSRuntime *rt, JSValue val)
{
	struct duk_pointer_ref *pref = JS_GetOpaque(val, duk_pointer_class_id);

	(void)rt;
	free(pref);
}
