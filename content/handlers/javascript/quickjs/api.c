/*
 * Slate stack-style JavaScript binding API backed by QuickJS.
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

#include "api.h"
#include "quickjs.h"

extern slatejs_bool_t slatejs_bind_check_timeout(void *udata);

struct slatejs_string_cache {
	char *data;
	slatejs_size_t len;
	struct slatejs_string_cache *next;
};

struct slatejs_cfunc_ref {
	slatejs_c_function func;
	slatejs_idx_t nargs;
};

struct slatejs_object_meta {
	slatejs_context *ctx;
	slatejs_c_function finalizer;
	void *private_ptr;
	struct slatejs_object_meta *next_finalizer;
};

struct slatejs_pointer_ref {
	void *ptr;
};

struct slatejs_context {
	JSRuntime *rt;
	JSContext *qctx;
	JSValue *stack;
	slatejs_idx_t top;
	slatejs_idx_t cap;
	JSValue global_object;
	JSValue current_this;
	slatejs_memory_functions memfuncs;
	char *pending_error;
	struct slatejs_string_cache *strings;
	struct slatejs_context *parent;
	struct slatejs_context *children;
	struct slatejs_context *next_child;
	struct slatejs_object_meta *finalizer_queue;
	struct slatejs_object_meta *finalizer_queue_tail;
	void *finalizer_private_ptr;
	bool draining_finalizers;
	bool owns_runtime;
};

static JSClassID slatejs_object_class_id;
static JSClassID slatejs_cfunc_class_id;
static JSClassID slatejs_pointer_class_id;
static JSClassID slatejs_thread_class_id;
static bool slatejs_class_ids_ready;
static const char slatejs_cfunc_ref_prop[] = "\xffSlateQuickJSCFunctionRef";
static const char slatejs_private_magic_prop[] = "\xff\xffSLATE_QUICKJS_PRIVATE";

static void slatejs_object_finalizer(JSRuntime *rt, JSValue val);
static void slatejs_cfunc_finalizer(JSRuntime *rt, JSValue val);
static JSValue slatejs_cfunc_call(JSContext *ctx, JSValueConst func_obj,
		JSValueConst this_val, int argc, JSValueConst *argv,
		int flags);
static JSValue slatejs_cfunc_data_call(JSContext *ctx, JSValueConst this_val,
		int argc, JSValueConst *argv, int magic, JSValue *func_data);
static struct slatejs_cfunc_ref *slatejs_get_cfunc_ref(slatejs_context *ctx,
		JSValueConst val);
static void slatejs_pointer_finalizer(JSRuntime *rt, JSValue val);
static JSValue slatejs_get_effective_global(slatejs_context *ctx);
static bool slatejs_is_effective_global_value(slatejs_context *ctx, JSValueConst val);
static void slatejs_sync_qjs_global(slatejs_context *ctx);
static void slatejs_drain_finalizers(slatejs_context *ctx);
static JSValue slatejs_call_c_function_value(slatejs_context *ctx,
		slatejs_c_function func, JSValueConst this_val, int argc,
		JSValueConst *argv);

static const JSClassDef slatejs_object_class = {
	.class_name = "SlateQuickJSObject",
	.finalizer = slatejs_object_finalizer,
};

static const JSClassDef slatejs_cfunc_class = {
	.class_name = "SlateQuickJSCFunction",
	.finalizer = slatejs_cfunc_finalizer,
	.call = slatejs_cfunc_call,
};

static const JSClassDef slatejs_pointer_class = {
	.class_name = "SlateQuickJSPointer",
	.finalizer = slatejs_pointer_finalizer,
};

static const JSClassDef slatejs_thread_class = {
	.class_name = "SlateQuickJSThread",
};

static void slatejs_init_class_ids(void)
{
	if (slatejs_class_ids_ready) {
		return;
	}

	JS_NewClassID(&slatejs_object_class_id);
	JS_NewClassID(&slatejs_cfunc_class_id);
	JS_NewClassID(&slatejs_pointer_class_id);
	JS_NewClassID(&slatejs_thread_class_id);
	slatejs_class_ids_ready = true;
}

static void slatejs_set_default_class_proto(JSContext *ctx, JSClassID class_id)
{
	JSValue obj = JS_NewObject(ctx);
	JSValue proto = JS_GetPrototype(ctx, obj);

	JS_FreeValue(ctx, obj);
	if (!JS_IsException(proto)) {
		JS_SetClassProto(ctx, class_id, proto);
	}
}

static bool slatejs_register_classes(JSRuntime *rt, JSContext *ctx)
{
	slatejs_init_class_ids();

	if (!JS_IsRegisteredClass(rt, slatejs_object_class_id) &&
			JS_NewClass(rt, slatejs_object_class_id, &slatejs_object_class) < 0) {
		return false;
	}
	if (!JS_IsRegisteredClass(rt, slatejs_cfunc_class_id) &&
			JS_NewClass(rt, slatejs_cfunc_class_id, &slatejs_cfunc_class) < 0) {
		return false;
	}
	if (!JS_IsRegisteredClass(rt, slatejs_pointer_class_id) &&
			JS_NewClass(rt, slatejs_pointer_class_id, &slatejs_pointer_class) < 0) {
		return false;
	}
	if (!JS_IsRegisteredClass(rt, slatejs_thread_class_id) &&
			JS_NewClass(rt, slatejs_thread_class_id, &slatejs_thread_class) < 0) {
		return false;
	}

	slatejs_set_default_class_proto(ctx, slatejs_object_class_id);
	slatejs_set_default_class_proto(ctx, slatejs_cfunc_class_id);
	slatejs_set_default_class_proto(ctx, slatejs_pointer_class_id);
	slatejs_set_default_class_proto(ctx, slatejs_thread_class_id);

	return true;
}

static void slatejs_clear_strings(slatejs_context *ctx)
{
	struct slatejs_string_cache *cur = ctx->strings;

	while (cur != NULL) {
		struct slatejs_string_cache *next = cur->next;
		free(cur->data);
		free(cur);
		cur = next;
	}
	ctx->strings = NULL;
}

static const char *slatejs_cache_string(slatejs_context *ctx, const char *data,
		slatejs_size_t len)
{
	struct slatejs_string_cache *cache;

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

static const char *slatejs_value_to_cached_string(slatejs_context *ctx, JSValueConst val,
		slatejs_size_t *out_len, bool safe)
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
		ret = slatejs_cache_string(ctx, "[conversion error]",
				sizeof("[conversion error]") - 1);
		if (out_len != NULL) {
			*out_len = sizeof("[conversion error]") - 1;
		}
		return ret;
	}

	ret = slatejs_cache_string(ctx, qstr, len);
	JS_FreeCString(ctx->qctx, qstr);
	if (out_len != NULL) {
		*out_len = len;
	}
	return ret;
}

static bool slatejs_reserve_stack(slatejs_context *ctx, slatejs_idx_t needed)
{
	JSValue *nstack;
	slatejs_idx_t ncap;

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

static bool slatejs_push_owned(slatejs_context *ctx, JSValue val)
{
	if (!slatejs_reserve_stack(ctx, ctx->top + 1)) {
		JS_FreeValue(ctx->qctx, val);
		return false;
	}

	ctx->stack[ctx->top++] = val;
	return true;
}

static slatejs_idx_t slatejs_resolve_index(slatejs_context *ctx, slatejs_idx_t idx)
{
	if (idx < 0) {
		idx = ctx->top + idx;
	}
	if (idx < 0 || idx >= ctx->top) {
		return -1;
	}
	return idx;
}

static JSValue slatejs_value_at(slatejs_context *ctx, slatejs_idx_t idx)
{
	idx = slatejs_resolve_index(ctx, idx);
	if (idx < 0) {
		return JS_UNDEFINED;
	}
	return ctx->stack[idx];
}

static JSValue *slatejs_value_slot(slatejs_context *ctx, slatejs_idx_t idx)
{
	idx = slatejs_resolve_index(ctx, idx);
	if (idx < 0) {
		return NULL;
	}
	return &ctx->stack[idx];
}

static void slatejs_free_range(slatejs_context *ctx, slatejs_idx_t start, slatejs_idx_t count)
{
	slatejs_idx_t i;

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

static int slatejs_qjs_type_to_duk(JSValueConst val)
{
	struct slatejs_pointer_ref *pref;

	if (JS_IsUndefined(val)) {
		return SLATEJS_TYPE_UNDEFINED;
	}
	if (JS_IsNull(val)) {
		return SLATEJS_TYPE_NULL;
	}
	if (JS_IsBool(val)) {
		return SLATEJS_TYPE_BOOLEAN;
	}
	if (JS_IsNumber(val)) {
		return SLATEJS_TYPE_NUMBER;
	}
	if (JS_IsString(val)) {
		return SLATEJS_TYPE_STRING;
	}
	pref = JS_GetOpaque(val, slatejs_pointer_class_id);
	if (pref != NULL) {
		return SLATEJS_TYPE_POINTER;
	}
	if (JS_IsObject(val)) {
		return SLATEJS_TYPE_OBJECT;
	}
	return SLATEJS_TYPE_NONE;
}

static void slatejs_set_pending_errorv(slatejs_context *ctx, const char *fmt, va_list ap)
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

static JSValue slatejs_throw_pending(slatejs_context *ctx)
{
	const char *msg = ctx->pending_error != NULL ?
			ctx->pending_error : "QuickJS compatibility error";
	return JS_ThrowTypeError(ctx->qctx, "%s", msg);
}

static JSValue slatejs_make_error(slatejs_context *ctx, const char *msg)
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

static JSAtom slatejs_atom_from_value(slatejs_context *ctx, JSValueConst key)
{
	struct slatejs_pointer_ref *pref = JS_GetOpaque(key, slatejs_pointer_class_id);

	if (pref != NULL) {
		char name[64];
		snprintf(name, sizeof(name), "\xffSLATE_PTR_%p", pref->ptr);
		return JS_NewAtom(ctx->qctx, name);
	}

	return JS_ValueToAtom(ctx->qctx, key);
}

static int slatejs_qjs_interrupt(JSRuntime *rt, void *opaque)
{
	slatejs_context *ctx = opaque;

	if (ctx == NULL) {
		return 0;
	}

	return slatejs_bind_check_timeout(ctx->memfuncs.udata) ? 1 : 0;
}

static void *slatejs_qjs_malloc(JSMallocState *s, size_t size)
{
	slatejs_context *ctx = s->opaque;

	if (ctx->memfuncs.alloc_func != NULL) {
		return ctx->memfuncs.alloc_func(ctx->memfuncs.udata, size);
	}
	return malloc(size);
}

static void slatejs_qjs_free(JSMallocState *s, void *ptr)
{
	slatejs_context *ctx = s->opaque;

	if (ctx->memfuncs.free_func != NULL) {
		ctx->memfuncs.free_func(ctx->memfuncs.udata, ptr);
		return;
	}
	free(ptr);
}

static void *slatejs_qjs_realloc(JSMallocState *s, void *ptr, size_t size)
{
	slatejs_context *ctx = s->opaque;

	if (ctx->memfuncs.realloc_func != NULL) {
		return ctx->memfuncs.realloc_func(ctx->memfuncs.udata, ptr, size);
	}
	return realloc(ptr, size);
}

static size_t slatejs_qjs_malloc_usable_size(const void *ptr)
{
#if defined(__linux__)
	return ptr != NULL ? malloc_usable_size((void *)ptr) : 0;
#else
	(void)ptr;
	return 0;
#endif
}

static const JSMallocFunctions slatejs_qjs_malloc_funcs = {
	.js_malloc = slatejs_qjs_malloc,
	.js_free = slatejs_qjs_free,
	.js_realloc = slatejs_qjs_realloc,
	.js_malloc_usable_size = slatejs_qjs_malloc_usable_size,
};

slatejs_context *slatejs_create_heap(slatejs_alloc_function alloc_func,
		slatejs_realloc_function realloc_func,
		slatejs_free_function free_func,
		void *heap_udata,
		slatejs_fatal_function fatal_handler)
{
	slatejs_context *ctx;

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

	ctx->rt = JS_NewRuntime2(&slatejs_qjs_malloc_funcs, ctx);
	if (ctx->rt == NULL) {
		free(ctx);
		return NULL;
	}

	JS_SetRuntimeOpaque(ctx->rt, ctx);
	JS_SetCanBlock(ctx->rt, 0);
	JS_SetInterruptHandler(ctx->rt, slatejs_qjs_interrupt, ctx);

	ctx->qctx = JS_NewContext(ctx->rt);
	if (ctx->qctx == NULL) {
		JS_FreeRuntime(ctx->rt);
		free(ctx);
		return NULL;
	}
	JS_SetContextOpaque(ctx->qctx, ctx);

	if (!slatejs_register_classes(ctx->rt, ctx->qctx)) {
		JS_FreeContext(ctx->qctx);
		JS_FreeRuntime(ctx->rt);
		free(ctx);
		return NULL;
	}

	return ctx;
}

static void slatejs_free_context_values(slatejs_context *ctx)
{
	slatejs_idx_t i;

	for (i = 0; i < ctx->top; i++) {
		JS_FreeValue(ctx->qctx, ctx->stack[i]);
	}
	free(ctx->stack);
	ctx->stack = NULL;
	ctx->top = 0;
	ctx->cap = 0;
	JS_FreeValue(ctx->qctx, ctx->current_this);
	ctx->current_this = JS_UNDEFINED;
	slatejs_clear_strings(ctx);
	free(ctx->pending_error);
	ctx->pending_error = NULL;
}

static void slatejs_free_child_contexts(slatejs_context *ctx)
{
	struct slatejs_context *child = ctx->children;

	while (child != NULL) {
		struct slatejs_context *next = child->next_child;
		slatejs_free_child_contexts(child);
		slatejs_free_context_values(child);
		slatejs_drain_finalizers(child);
		JS_FreeValue(child->qctx, child->global_object);
		child->global_object = JS_UNDEFINED;
		slatejs_drain_finalizers(child);
		child = next;
	}
}

static void slatejs_free_child_context_structs(slatejs_context *ctx)
{
	struct slatejs_context *child = ctx->children;

	while (child != NULL) {
		struct slatejs_context *next = child->next_child;
		slatejs_free_child_context_structs(child);
		free(child);
		child = next;
	}
	ctx->children = NULL;
}

static JSValue slatejs_get_effective_global(slatejs_context *ctx)
{
	if (!JS_IsUndefined(ctx->global_object)) {
		return JS_DupValue(ctx->qctx, ctx->global_object);
	}

	return JS_GetGlobalObject(ctx->qctx);
}

static bool slatejs_is_effective_global_value(slatejs_context *ctx, JSValueConst val)
{
	return !JS_IsUndefined(ctx->global_object) &&
		JS_IsObject(val) &&
		JS_VALUE_GET_TAG(val) == JS_VALUE_GET_TAG(ctx->global_object) &&
		JS_VALUE_GET_PTR(val) == JS_VALUE_GET_PTR(ctx->global_object);
}

static void slatejs_sync_qjs_global(slatejs_context *ctx)
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

static slatejs_context *slatejs_root_context(slatejs_context *ctx)
{
	while (ctx != NULL && ctx->parent != NULL) {
		ctx = ctx->parent;
	}
	return ctx;
}

static void slatejs_queue_finalizer(struct slatejs_object_meta *meta)
{
	slatejs_context *root;

	if (meta == NULL) {
		return;
	}
	if (meta->ctx == NULL || meta->finalizer == NULL ||
			meta->private_ptr == NULL) {
		free(meta);
		return;
	}

	root = slatejs_root_context(meta->ctx);
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

static void slatejs_drain_finalizers(slatejs_context *ctx)
{
	slatejs_context *root = slatejs_root_context(ctx);

	if (root == NULL || root->draining_finalizers) {
		return;
	}

	root->draining_finalizers = true;
	while (root->finalizer_queue != NULL) {
		struct slatejs_object_meta *meta = root->finalizer_queue;
		slatejs_context *owner = meta->ctx;
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
		result = slatejs_call_c_function_value(owner, meta->finalizer,
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

static void slatejs_collect_and_drain(slatejs_context *ctx)
{
	slatejs_context *root = slatejs_root_context(ctx);
	int i;

	if (root == NULL) {
		return;
	}

	for (i = 0; i < 4; i++) {
		JS_RunGC(root->rt);
		slatejs_drain_finalizers(root);
		if (root->finalizer_queue == NULL) {
			break;
		}
	}
}

static void slatejs_detach_qjs_global(slatejs_context *ctx)
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

void slatejs_destroy_heap(slatejs_context *ctx)
{
	if (ctx == NULL) {
		return;
	}

	slatejs_free_child_contexts(ctx);
	slatejs_free_context_values(ctx);
	slatejs_drain_finalizers(ctx);
	slatejs_detach_qjs_global(ctx);
	JS_FreeValue(ctx->qctx, ctx->global_object);
	ctx->global_object = JS_UNDEFINED;
	slatejs_collect_and_drain(ctx);
	slatejs_free_child_context_structs(ctx);
	JS_FreeContext(ctx->qctx);
	JS_FreeRuntime(ctx->rt);
	free(ctx);
}

void slatejs_gc(slatejs_context *ctx, slatejs_uint_t flags)
{
	(void)flags;
	slatejs_collect_and_drain(ctx);
}

slatejs_int_t slatejs_execute_pending_jobs(slatejs_context *ctx,
		slatejs_uint_t max_jobs)
{
	slatejs_context *root = slatejs_root_context(ctx);
	JSContext *job_ctx = NULL;
	void *old_opaque;
	slatejs_uint_t count = 0;

	if (root == NULL) {
		return SLATEJS_EXEC_SUCCESS;
	}

	old_opaque = JS_GetContextOpaque(ctx->qctx);
	slatejs_sync_qjs_global(ctx);

	while (max_jobs == 0 || count < max_jobs) {
		int ret = JS_ExecutePendingJob(root->rt, &job_ctx);

		if (ret == 0) {
			break;
		}

		if (ret < 0) {
			JSContext *err_ctx = job_ctx != NULL ? job_ctx : ctx->qctx;
			JSValue exception = JS_GetException(err_ctx);

			JS_SetContextOpaque(ctx->qctx, old_opaque);
			slatejs_push_owned(ctx, exception);
			slatejs_drain_finalizers(ctx);
			return SLATEJS_EXEC_ERROR;
		}

		count++;
	}

	JS_SetContextOpaque(ctx->qctx, old_opaque);
	slatejs_drain_finalizers(ctx);
	return SLATEJS_EXEC_SUCCESS;
}

void slatejs_get_memory_functions(slatejs_context *ctx, slatejs_memory_functions *out_funcs)
{
	if (out_funcs != NULL) {
		*out_funcs = ctx->memfuncs;
	}
}

slatejs_idx_t slatejs_get_top(slatejs_context *ctx)
{
	return ctx->top;
}

void slatejs_set_top(slatejs_context *ctx, slatejs_idx_t top)
{
	slatejs_idx_t i;

	slatejs_clear_strings(ctx);
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

	if (!slatejs_reserve_stack(ctx, top)) {
		return;
	}
	while (ctx->top < top) {
		ctx->stack[ctx->top++] = JS_UNDEFINED;
	}
}

slatejs_idx_t slatejs_normalize_index(slatejs_context *ctx, slatejs_idx_t idx)
{
	return slatejs_resolve_index(ctx, idx);
}

void slatejs_pop(slatejs_context *ctx)
{
	slatejs_pop_n(ctx, 1);
}

void slatejs_pop_2(slatejs_context *ctx)
{
	slatejs_pop_n(ctx, 2);
}

void slatejs_pop_3(slatejs_context *ctx)
{
	slatejs_pop_n(ctx, 3);
}

void slatejs_pop_n(slatejs_context *ctx, slatejs_idx_t count)
{
	if (count <= 0) {
		return;
	}
	if (count > ctx->top) {
		count = ctx->top;
	}
	slatejs_set_top(ctx, ctx->top - count);
}

void slatejs_dup(slatejs_context *ctx, slatejs_idx_t idx)
{
	idx = slatejs_resolve_index(ctx, idx);
	if (idx < 0) {
		slatejs_push_undefined(ctx);
		return;
	}
	slatejs_push_owned(ctx, JS_DupValue(ctx->qctx, ctx->stack[idx]));
}

void slatejs_dup_top(slatejs_context *ctx)
{
	slatejs_dup(ctx, -1);
}

void slatejs_insert(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue val;

	idx = slatejs_resolve_index(ctx, idx);
	if (idx < 0 || ctx->top == 0 || idx == ctx->top - 1) {
		return;
	}

	val = ctx->stack[ctx->top - 1];
	memmove(&ctx->stack[idx + 1], &ctx->stack[idx],
			sizeof(*ctx->stack) * (ctx->top - idx - 1));
	ctx->stack[idx] = val;
}

void slatejs_remove(slatejs_context *ctx, slatejs_idx_t idx)
{
	idx = slatejs_resolve_index(ctx, idx);
	if (idx < 0) {
		return;
	}
	slatejs_free_range(ctx, idx, 1);
}

void slatejs_replace(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue val;

	idx = slatejs_resolve_index(ctx, idx);
	if (idx < 0 || ctx->top == 0) {
		return;
	}
	val = ctx->stack[ctx->top - 1];
	ctx->top--;
	JS_FreeValue(ctx->qctx, ctx->stack[idx]);
	ctx->stack[idx] = val;
}

void slatejs_push_undefined(slatejs_context *ctx)
{
	slatejs_push_owned(ctx, JS_UNDEFINED);
}

void slatejs_push_null(slatejs_context *ctx)
{
	slatejs_push_owned(ctx, JS_NULL);
}

void slatejs_push_boolean(slatejs_context *ctx, slatejs_bool_t val)
{
	slatejs_push_owned(ctx, JS_NewBool(ctx->qctx, val));
}

void slatejs_push_int(slatejs_context *ctx, slatejs_int_t val)
{
	slatejs_push_owned(ctx, JS_NewInt32(ctx->qctx, val));
}

void slatejs_push_uint(slatejs_context *ctx, slatejs_uint_t val)
{
	slatejs_push_owned(ctx, JS_NewUint32(ctx->qctx, val));
}

void slatejs_push_number(slatejs_context *ctx, slatejs_double_t val)
{
	slatejs_push_owned(ctx, JS_NewFloat64(ctx->qctx, val));
}

void slatejs_push_string(slatejs_context *ctx, const char *str)
{
	slatejs_push_owned(ctx, str != NULL ?
			JS_NewString(ctx->qctx, str) : JS_NULL);
}

void slatejs_push_lstring(slatejs_context *ctx, const char *str, slatejs_size_t len)
{
	if (str == NULL) {
		slatejs_push_null(ctx);
		return;
	}
	slatejs_push_owned(ctx, JS_NewStringLen(ctx->qctx, str, len));
}

void slatejs_push_pointer(slatejs_context *ctx, void *ptr)
{
	struct slatejs_pointer_ref *pref;
	JSValue obj;

	pref = calloc(1, sizeof(*pref));
	if (pref == NULL) {
		slatejs_push_undefined(ctx);
		return;
	}
	pref->ptr = ptr;
	obj = JS_NewObjectClass(ctx->qctx, slatejs_pointer_class_id);
	if (JS_IsException(obj)) {
		free(pref);
		slatejs_push_owned(ctx, JS_GetException(ctx->qctx));
		return;
	}
	JS_SetOpaque(obj, pref);
	slatejs_push_owned(ctx, obj);
}

slatejs_idx_t slatejs_push_object(slatejs_context *ctx)
{
	struct slatejs_object_meta *meta;
	JSValue obj;

	meta = calloc(1, sizeof(*meta));
	if (meta == NULL) {
		slatejs_push_undefined(ctx);
		return ctx->top - 1;
	}
	meta->ctx = ctx;
	obj = JS_NewObjectClass(ctx->qctx, slatejs_object_class_id);
	if (JS_IsException(obj)) {
		free(meta);
		slatejs_push_owned(ctx, JS_GetException(ctx->qctx));
		return ctx->top - 1;
	}
	JS_SetOpaque(obj, meta);
	slatejs_push_owned(ctx, obj);
	return ctx->top - 1;
}

slatejs_idx_t slatejs_push_array(slatejs_context *ctx)
{
	slatejs_push_owned(ctx, JS_NewArray(ctx->qctx));
	return ctx->top - 1;
}

static void slatejs_free_array_buffer(JSRuntime *rt, void *opaque, void *ptr)
{
	(void)rt;
	(void)opaque;
	free(ptr);
}

void *slatejs_push_buffer(slatejs_context *ctx, slatejs_size_t size, slatejs_bool_t dynamic)
{
	void *buf;

	(void)dynamic;
	buf = calloc(1, size == 0 ? 1 : size);
	if (buf == NULL) {
		slatejs_push_undefined(ctx);
		return NULL;
	}

	slatejs_push_owned(ctx, JS_NewArrayBuffer(ctx->qctx, buf, size,
			slatejs_free_array_buffer, NULL, false));
	return buf;
}

void slatejs_push_buffer_object(slatejs_context *ctx, slatejs_idx_t idx,
		slatejs_size_t byte_offset, slatejs_size_t byte_length,
		slatejs_uint_t flags)
{
	JSValue abuf = slatejs_value_at(ctx, idx);
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
	slatejs_push_owned(ctx, view);
}

slatejs_idx_t slatejs_push_c_function(slatejs_context *ctx, slatejs_c_function func,
		slatejs_idx_t nargs)
{
	struct slatejs_cfunc_ref *ref;
	JSValue ref_obj;
	JSValue obj;
	JSValue data[1];
	int qjs_nargs = nargs < 0 ? 0 : nargs;

	ref = calloc(1, sizeof(*ref));
	if (ref == NULL) {
		slatejs_push_undefined(ctx);
		return ctx->top - 1;
	}
	ref->func = func;
	ref->nargs = nargs;

	ref_obj = JS_NewObjectClass(ctx->qctx, slatejs_cfunc_class_id);
	if (JS_IsException(ref_obj)) {
		free(ref);
		slatejs_push_owned(ctx, JS_GetException(ctx->qctx));
		return ctx->top - 1;
	}
	JS_SetOpaque(ref_obj, ref);

	data[0] = ref_obj;
	obj = JS_NewCFunctionData(ctx->qctx, slatejs_cfunc_data_call,
			qjs_nargs, 0, 1, data);
	if (JS_IsException(obj)) {
		JS_FreeValue(ctx->qctx, ref_obj);
		slatejs_push_owned(ctx, JS_GetException(ctx->qctx));
		return ctx->top - 1;
	}

	JS_SetConstructorBit(ctx->qctx, obj, true);
	JS_DefinePropertyValueStr(ctx->qctx, obj, slatejs_cfunc_ref_prop,
			JS_DupValue(ctx->qctx, ref_obj),
			JS_PROP_CONFIGURABLE);
	JS_FreeValue(ctx->qctx, ref_obj);
	slatejs_push_owned(ctx, obj);
	return ctx->top - 1;
}

void slatejs_push_global_object(slatejs_context *ctx)
{
	slatejs_push_owned(ctx, slatejs_get_effective_global(ctx));
}

void slatejs_push_this(slatejs_context *ctx)
{
	slatejs_push_owned(ctx, JS_DupValue(ctx->qctx, ctx->current_this));
}

void slatejs_push_thread(slatejs_context *ctx)
{
	slatejs_context *child;
	JSValue obj;

	child = calloc(1, sizeof(*child));
	if (child == NULL) {
		slatejs_push_undefined(ctx);
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

	obj = JS_NewObjectClass(ctx->qctx, slatejs_thread_class_id);
	if (JS_IsException(obj)) {
		slatejs_push_owned(ctx, JS_GetException(ctx->qctx));
		return;
	}
	JS_SetOpaque(obj, child);
	slatejs_push_owned(ctx, obj);
}

slatejs_idx_t slatejs_push_error_object(slatejs_context *ctx, slatejs_errcode_t err_code,
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

	slatejs_push_owned(ctx, slatejs_make_error(ctx, msg != NULL ? msg : ""));
	free(msg);
	return ctx->top - 1;
}

void slatejs_push_sprintf(slatejs_context *ctx, const char *fmt, ...)
{
	va_list ap;
	char *msg = NULL;
	int len;

	va_start(ap, fmt);
	len = vsnprintf(NULL, 0, fmt, ap);
	va_end(ap);
	if (len < 0) {
		slatejs_push_string(ctx, "");
		return;
	}

	msg = malloc((size_t)len + 1);
	if (msg == NULL) {
		slatejs_push_string(ctx, "");
		return;
	}

	va_start(ap, fmt);
	vsnprintf(msg, (size_t)len + 1, fmt, ap);
	va_end(ap);
	slatejs_push_lstring(ctx, msg, (slatejs_size_t)len);
	free(msg);
}

void slatejs_push_context_dump(slatejs_context *ctx)
{
	char buf[64];

	snprintf(buf, sizeof(buf), "[qjs stack top=%d]", ctx->top);
	slatejs_push_string(ctx, buf);
}

slatejs_context *slatejs_require_context(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue val = slatejs_value_at(ctx, idx);
	slatejs_context *child = JS_GetOpaque(val, slatejs_thread_class_id);

	return child;
}

slatejs_bool_t slatejs_get_prop(slatejs_context *ctx, slatejs_idx_t obj_idx)
{
	slatejs_idx_t obj_pos = slatejs_resolve_index(ctx, obj_idx);
	JSAtom atom;
	JSValue val;
	bool exists;

	if (obj_pos < 0 || ctx->top < 1) {
		slatejs_push_undefined(ctx);
		return false;
	}

	atom = slatejs_atom_from_value(ctx, ctx->stack[ctx->top - 1]);
	JS_FreeValue(ctx->qctx, ctx->stack[ctx->top - 1]);
	ctx->top--;
	if (atom == JS_ATOM_NULL) {
		slatejs_push_undefined(ctx);
		return false;
	}

	val = JS_GetProperty(ctx->qctx, ctx->stack[obj_pos], atom);
	JS_FreeAtom(ctx->qctx, atom);
	exists = !JS_IsUndefined(val);
	slatejs_push_owned(ctx, val);
	return exists;
}

slatejs_bool_t slatejs_get_prop_string(slatejs_context *ctx, slatejs_idx_t obj_idx,
		const char *key)
{
	slatejs_idx_t obj_pos = slatejs_resolve_index(ctx, obj_idx);
	JSValue obj = obj_pos >= 0 ? ctx->stack[obj_pos] : JS_UNDEFINED;
	JSValue val;
	bool exists;

	if (ctx->finalizer_private_ptr != NULL && obj_pos == 0 &&
			strcmp(key, slatejs_private_magic_prop) == 0) {
		slatejs_push_pointer(ctx, ctx->finalizer_private_ptr);
		return true;
	}

	val = JS_GetPropertyStr(ctx->qctx, obj, key);
	if (JS_IsUndefined(val) && slatejs_is_effective_global_value(ctx, obj)) {
		JSValue global = JS_GetGlobalObject(ctx->qctx);

		JS_FreeValue(ctx->qctx, val);
		val = JS_GetPropertyStr(ctx->qctx, global, key);
		JS_FreeValue(ctx->qctx, global);
	}
	exists = !JS_IsUndefined(val);

	slatejs_push_owned(ctx, val);
	return exists;
}

slatejs_bool_t slatejs_get_prop_index(slatejs_context *ctx, slatejs_idx_t obj_idx,
		slatejs_uarridx_t idx)
{
	JSValue obj = slatejs_value_at(ctx, obj_idx);
	JSValue val = JS_GetPropertyUint32(ctx->qctx, obj, idx);
	bool exists = !JS_IsUndefined(val);

	slatejs_push_owned(ctx, val);
	return exists;
}

slatejs_bool_t slatejs_get_global_string(slatejs_context *ctx, const char *key)
{
	JSValue global = slatejs_get_effective_global(ctx);
	JSValue val = JS_GetPropertyStr(ctx->qctx, global, key);
	bool exists = !JS_IsUndefined(val);

	JS_FreeValue(ctx->qctx, global);
	slatejs_push_owned(ctx, val);
	return exists;
}

slatejs_bool_t slatejs_put_prop(slatejs_context *ctx, slatejs_idx_t obj_idx)
{
	slatejs_idx_t obj_pos = slatejs_resolve_index(ctx, obj_idx);
	JSAtom atom;
	JSValue key;
	JSValue val;
	int ret;

	if (obj_pos < 0 || ctx->top < 2) {
		return false;
	}

	key = ctx->stack[ctx->top - 2];
	val = ctx->stack[ctx->top - 1];
	atom = slatejs_atom_from_value(ctx, key);
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

slatejs_bool_t slatejs_put_prop_string(slatejs_context *ctx, slatejs_idx_t obj_idx,
		const char *key)
{
	slatejs_idx_t obj_pos = slatejs_resolve_index(ctx, obj_idx);
	struct slatejs_object_meta *meta;
	struct slatejs_pointer_ref *pref;
	JSValue val;
	int ret;

	if (obj_pos < 0 || ctx->top < 1) {
		return false;
	}

	val = ctx->stack[ctx->top - 1];
	ctx->top--;
	meta = JS_GetOpaque(ctx->stack[obj_pos], slatejs_object_class_id);
	pref = JS_GetOpaque(val, slatejs_pointer_class_id);
	if (meta != NULL && pref != NULL &&
			strcmp(key, slatejs_private_magic_prop) == 0) {
		meta->private_ptr = pref->ptr;
	}
	ret = JS_SetPropertyStr(ctx->qctx, ctx->stack[obj_pos], key, val);
	return ret >= 0;
}

slatejs_bool_t slatejs_put_prop_index(slatejs_context *ctx, slatejs_idx_t obj_idx,
		slatejs_uarridx_t idx)
{
	slatejs_idx_t obj_pos = slatejs_resolve_index(ctx, obj_idx);
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

slatejs_bool_t slatejs_put_global_string(slatejs_context *ctx, const char *key)
{
	JSValue global;
	JSValue val;
	int ret;

	if (ctx->top < 1) {
		return false;
	}

	global = slatejs_get_effective_global(ctx);
	val = ctx->stack[ctx->top - 1];
	ctx->top--;
	ret = JS_SetPropertyStr(ctx->qctx, global, key, val);
	JS_FreeValue(ctx->qctx, global);
	return ret >= 0;
}

slatejs_bool_t slatejs_del_prop(slatejs_context *ctx, slatejs_idx_t obj_idx)
{
	slatejs_idx_t obj_pos = slatejs_resolve_index(ctx, obj_idx);
	JSAtom atom;
	JSValue key;
	int ret;

	if (obj_pos < 0 || ctx->top < 1) {
		return false;
	}

	key = ctx->stack[ctx->top - 1];
	atom = slatejs_atom_from_value(ctx, key);
	JS_FreeValue(ctx->qctx, key);
	ctx->top--;
	if (atom == JS_ATOM_NULL) {
		return false;
	}

	ret = JS_DeleteProperty(ctx->qctx, ctx->stack[obj_pos], atom, 0);
	JS_FreeAtom(ctx->qctx, atom);
	return ret >= 0;
}

slatejs_bool_t slatejs_del_prop_string(slatejs_context *ctx, slatejs_idx_t obj_idx,
		const char *key)
{
	JSAtom atom;
	int ret;
	JSValue obj = slatejs_value_at(ctx, obj_idx);

	atom = JS_NewAtom(ctx->qctx, key);
	if (atom == JS_ATOM_NULL) {
		return false;
	}
	ret = JS_DeleteProperty(ctx->qctx, obj, atom, 0);
	JS_FreeAtom(ctx->qctx, atom);
	return ret >= 0;
}

slatejs_bool_t slatejs_del_prop_index(slatejs_context *ctx, slatejs_idx_t obj_idx,
		slatejs_uarridx_t idx)
{
	JSAtom atom;
	int ret;
	JSValue obj = slatejs_value_at(ctx, obj_idx);

	atom = JS_NewAtomUInt32(ctx->qctx, idx);
	if (atom == JS_ATOM_NULL) {
		return false;
	}
	ret = JS_DeleteProperty(ctx->qctx, obj, atom, 0);
	JS_FreeAtom(ctx->qctx, atom);
	return ret >= 0;
}

slatejs_bool_t slatejs_has_prop(slatejs_context *ctx, slatejs_idx_t obj_idx)
{
	slatejs_idx_t obj_pos = slatejs_resolve_index(ctx, obj_idx);
	JSAtom atom;
	JSValue key;
	int ret;

	if (obj_pos < 0 || ctx->top < 1) {
		return false;
	}

	key = ctx->stack[ctx->top - 1];
	atom = slatejs_atom_from_value(ctx, key);
	JS_FreeValue(ctx->qctx, key);
	ctx->top--;
	if (atom == JS_ATOM_NULL) {
		return false;
	}
	ret = JS_HasProperty(ctx->qctx, ctx->stack[obj_pos], atom);
	JS_FreeAtom(ctx->qctx, atom);
	return ret > 0;
}

static int slatejs_defprop_flags_to_qjs(slatejs_uint_t flags)
{
	int qflags = 0;

	if (flags & SLATEJS_DEFPROP_HAVE_VALUE) {
		qflags |= JS_PROP_HAS_VALUE;
	}
	if (flags & SLATEJS_DEFPROP_HAVE_WRITABLE) {
		qflags |= JS_PROP_HAS_WRITABLE;
		if (flags & SLATEJS_DEFPROP_WRITABLE) {
			qflags |= JS_PROP_WRITABLE;
		}
	}
	if (flags & SLATEJS_DEFPROP_HAVE_ENUMERABLE) {
		qflags |= JS_PROP_HAS_ENUMERABLE;
		if (flags & SLATEJS_DEFPROP_ENUMERABLE) {
			qflags |= JS_PROP_ENUMERABLE;
		}
	}
	if (flags & SLATEJS_DEFPROP_HAVE_CONFIGURABLE) {
		qflags |= JS_PROP_HAS_CONFIGURABLE;
		if (flags & SLATEJS_DEFPROP_CONFIGURABLE) {
			qflags |= JS_PROP_CONFIGURABLE;
		}
	}
	if (flags & SLATEJS_DEFPROP_HAVE_GETTER) {
		qflags |= JS_PROP_HAS_GET;
	}
	if (flags & SLATEJS_DEFPROP_HAVE_SETTER) {
		qflags |= JS_PROP_HAS_SET;
	}

	return qflags;
}

void slatejs_def_prop(slatejs_context *ctx, slatejs_idx_t obj_idx, slatejs_uint_t flags)
{
	slatejs_idx_t obj_pos = slatejs_resolve_index(ctx, obj_idx);
	slatejs_idx_t nvals = 0;
	slatejs_idx_t key_pos;
	JSAtom atom;
	JSValue value = JS_UNDEFINED;
	JSValue getter = JS_UNDEFINED;
	JSValue setter = JS_UNDEFINED;

	if (obj_pos < 0) {
		return;
	}

	if (flags & SLATEJS_DEFPROP_HAVE_VALUE) {
		nvals++;
	}
	if (flags & SLATEJS_DEFPROP_HAVE_GETTER) {
		nvals++;
	}
	if (flags & SLATEJS_DEFPROP_HAVE_SETTER) {
		nvals++;
	}
	if (ctx->top < nvals + 1) {
		return;
	}

	key_pos = ctx->top - nvals - 1;
	atom = slatejs_atom_from_value(ctx, ctx->stack[key_pos]);
	if (atom == JS_ATOM_NULL) {
		slatejs_pop_n(ctx, nvals + 1);
		return;
	}

	if (flags & SLATEJS_DEFPROP_HAVE_VALUE) {
		value = JS_DupValue(ctx->qctx, ctx->stack[key_pos + 1]);
	} else if (flags & SLATEJS_DEFPROP_HAVE_GETTER) {
		getter = JS_DupValue(ctx->qctx, ctx->stack[key_pos + 1]);
		if (flags & SLATEJS_DEFPROP_HAVE_SETTER) {
			setter = JS_DupValue(ctx->qctx, ctx->stack[key_pos + 2]);
		}
	}

	JS_DefineProperty(ctx->qctx, ctx->stack[obj_pos], atom, value,
			getter, setter, slatejs_defprop_flags_to_qjs(flags));
	JS_FreeAtom(ctx->qctx, atom);
	JS_FreeValue(ctx->qctx, value);
	JS_FreeValue(ctx->qctx, getter);
	JS_FreeValue(ctx->qctx, setter);
	slatejs_pop_n(ctx, nvals + 1);
}

void slatejs_set_prototype(slatejs_context *ctx, slatejs_idx_t idx)
{
	slatejs_idx_t obj_pos = slatejs_resolve_index(ctx, idx);
	JSValue proto;
	struct slatejs_object_meta *obj_meta;
	struct slatejs_object_meta *proto_meta;

	if (obj_pos < 0 || ctx->top < 1) {
		return;
	}

	proto = ctx->stack[ctx->top - 1];
	obj_meta = JS_GetOpaque(ctx->stack[obj_pos], slatejs_object_class_id);
	proto_meta = JS_GetOpaque(proto, slatejs_object_class_id);
	if (obj_meta != NULL && proto_meta != NULL &&
			obj_meta->finalizer == NULL) {
		obj_meta->finalizer = proto_meta->finalizer;
	}
	JS_SetPrototype(ctx->qctx, ctx->stack[obj_pos], proto);
	JS_FreeValue(ctx->qctx, proto);
	ctx->top--;
}

void slatejs_get_prototype(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue proto = JS_GetPrototype(ctx->qctx, slatejs_value_at(ctx, idx));

	if (JS_IsNull(proto)) {
		JS_FreeValue(ctx->qctx, proto);
		proto = JS_UNDEFINED;
	}
	slatejs_push_owned(ctx, proto);
}

void slatejs_set_finalizer(slatejs_context *ctx, slatejs_idx_t idx)
{
	slatejs_idx_t obj_pos = slatejs_resolve_index(ctx, idx);
	struct slatejs_object_meta *meta;
	struct slatejs_cfunc_ref *ref = NULL;

	if (obj_pos < 0 || ctx->top < 1) {
		return;
	}

	meta = JS_GetOpaque(ctx->stack[obj_pos], slatejs_object_class_id);
	ref = slatejs_get_cfunc_ref(ctx, ctx->stack[ctx->top - 1]);
	if (meta != NULL) {
		meta->finalizer = ref != NULL ? ref->func : NULL;
	}
	slatejs_pop(ctx);
}

void slatejs_set_global_object(slatejs_context *ctx)
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

static void slatejs_frame_enter(slatejs_context *ctx, JSValueConst this_val,
		int argc, JSValueConst *argv, JSValue **old_stack,
		slatejs_idx_t *old_top, slatejs_idx_t *old_cap, JSValue *old_this)
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
	slatejs_clear_strings(ctx);
	free(ctx->pending_error);
	ctx->pending_error = NULL;

	slatejs_reserve_stack(ctx, argc);
	for (i = 0; i < argc; i++) {
		ctx->stack[ctx->top++] = JS_DupValue(ctx->qctx, argv[i]);
	}
}

static void slatejs_frame_leave(slatejs_context *ctx, JSValue *old_stack,
		slatejs_idx_t old_top, slatejs_idx_t old_cap, JSValue old_this)
{
	slatejs_free_context_values(ctx);
	ctx->stack = old_stack;
	ctx->top = old_top;
	ctx->cap = old_cap;
	ctx->current_this = old_this;
}

static JSValue slatejs_call_c_function_value(slatejs_context *ctx,
		slatejs_c_function func, JSValueConst this_val, int argc,
		JSValueConst *argv)
{
	JSValue *old_stack;
	JSValue old_this;
	slatejs_idx_t old_top;
	slatejs_idx_t old_cap;
	slatejs_ret_t ret;
	JSValue result = JS_UNDEFINED;

	slatejs_frame_enter(ctx, this_val, argc, argv, &old_stack, &old_top,
			&old_cap, &old_this);

	ret = func(ctx);
	if (ret < 0) {
		result = slatejs_throw_pending(ctx);
	} else if (ret > 0 && ctx->top >= ret) {
		result = ctx->stack[ctx->top - ret];
		ctx->stack[ctx->top - ret] = JS_UNDEFINED;
	} else {
		result = JS_UNDEFINED;
	}

	slatejs_frame_leave(ctx, old_stack, old_top, old_cap, old_this);
	slatejs_drain_finalizers(ctx);
	return result;
}

static JSValue slatejs_cfunc_call(JSContext *qctx, JSValueConst func_obj,
		JSValueConst this_val, int argc, JSValueConst *argv,
		int flags)
{
	struct slatejs_cfunc_ref *ref = JS_GetOpaque(func_obj, slatejs_cfunc_class_id);
	slatejs_context *ctx = JS_GetContextOpaque(qctx);

	(void)flags;

	if (ctx == NULL || ref == NULL || ref->func == NULL) {
		return JS_ThrowTypeError(qctx, "invalid Slate C function");
	}

	return slatejs_call_c_function_value(ctx, ref->func, this_val, argc, argv);
}

static JSValue slatejs_cfunc_data_call(JSContext *qctx, JSValueConst this_val,
		int argc, JSValueConst *argv, int magic, JSValue *func_data)
{
	struct slatejs_cfunc_ref *ref = JS_GetOpaque(func_data[0],
			slatejs_cfunc_class_id);
	slatejs_context *ctx = JS_GetContextOpaque(qctx);

	(void)magic;

	if (ctx == NULL || ref == NULL || ref->func == NULL) {
		return JS_ThrowTypeError(qctx, "invalid Slate C function");
	}

	return slatejs_call_c_function_value(ctx, ref->func, this_val, argc, argv);
}

static struct slatejs_cfunc_ref *slatejs_get_cfunc_ref(slatejs_context *ctx,
		JSValueConst val)
{
	struct slatejs_cfunc_ref *ref;
	JSValue ref_obj;

	ref = JS_GetOpaque(val, slatejs_cfunc_class_id);
	if (ref != NULL) {
		return ref;
	}

	ref_obj = JS_GetPropertyStr(ctx->qctx, val, slatejs_cfunc_ref_prop);
	if (JS_IsException(ref_obj)) {
		JS_FreeValue(ctx->qctx, JS_GetException(ctx->qctx));
		return NULL;
	}
	ref = JS_GetOpaque(ref_obj, slatejs_cfunc_class_id);
	JS_FreeValue(ctx->qctx, ref_obj);
	return ref;
}

static void slatejs_replace_call_values(slatejs_context *ctx, slatejs_idx_t start,
		slatejs_idx_t count, JSValue result)
{
	slatejs_free_range(ctx, start, count);
	slatejs_push_owned(ctx, result);
}

static slatejs_int_t slatejs_call_js(slatejs_context *ctx, slatejs_idx_t func_pos,
		JSValueConst this_val, slatejs_idx_t nargs, slatejs_idx_t consume_count)
{
	JSValue func = ctx->stack[func_pos];
	JSValue result;
	void *old_opaque;

	old_opaque = JS_GetContextOpaque(ctx->qctx);
	slatejs_sync_qjs_global(ctx);

	if (JS_VALUE_GET_TAG(func) == JS_TAG_FUNCTION_BYTECODE) {
		result = JS_EvalFunction(ctx->qctx, JS_DupValue(ctx->qctx, func));
	} else {
		result = JS_Call(ctx->qctx, func, this_val, nargs,
				(JSValueConst *)&ctx->stack[func_pos + consume_count - nargs]);
	}

	if (JS_IsException(result)) {
		result = JS_GetException(ctx->qctx);
		JS_SetContextOpaque(ctx->qctx, old_opaque);
		slatejs_replace_call_values(ctx, func_pos, consume_count, result);
		slatejs_drain_finalizers(ctx);
		return SLATEJS_EXEC_ERROR;
	}

	slatejs_replace_call_values(ctx, func_pos, consume_count, result);

	if (slatejs_execute_pending_jobs(ctx, 1024) == SLATEJS_EXEC_ERROR) {
		JSValue exception = ctx->stack[ctx->top - 1];

		ctx->stack[ctx->top - 1] = JS_UNDEFINED;
		slatejs_free_range(ctx, func_pos, ctx->top - func_pos);
		ctx->top = func_pos;
		slatejs_push_owned(ctx, exception);
		JS_SetContextOpaque(ctx->qctx, old_opaque);
		slatejs_drain_finalizers(ctx);
		return SLATEJS_EXEC_ERROR;
	}

	JS_SetContextOpaque(ctx->qctx, old_opaque);
	slatejs_drain_finalizers(ctx);
	return SLATEJS_EXEC_SUCCESS;
}

void slatejs_call(slatejs_context *ctx, slatejs_idx_t nargs)
{
	slatejs_idx_t func_pos = ctx->top - nargs - 1;

	if (func_pos < 0) {
		slatejs_push_undefined(ctx);
		return;
	}
	(void)slatejs_call_js(ctx, func_pos, JS_UNDEFINED, nargs, nargs + 1);
}

slatejs_int_t slatejs_pcall(slatejs_context *ctx, slatejs_idx_t nargs)
{
	slatejs_idx_t func_pos = ctx->top - nargs - 1;

	if (func_pos < 0) {
		slatejs_push_undefined(ctx);
		return SLATEJS_EXEC_ERROR;
	}
	return slatejs_call_js(ctx, func_pos, JS_UNDEFINED, nargs, nargs + 1);
}

slatejs_int_t slatejs_pcall_method(slatejs_context *ctx, slatejs_idx_t nargs)
{
	slatejs_idx_t func_pos = ctx->top - nargs - 2;
	JSValue this_val;

	if (func_pos < 0) {
		slatejs_push_undefined(ctx);
		return SLATEJS_EXEC_ERROR;
	}

	this_val = ctx->stack[func_pos + 1];
	return slatejs_call_js(ctx, func_pos, this_val, nargs, nargs + 2);
}

slatejs_int_t slatejs_safe_call(slatejs_context *ctx, slatejs_safe_call_function func,
		void *udata, slatejs_idx_t nargs, slatejs_idx_t nrets)
{
	slatejs_idx_t base = ctx->top - nargs;
	JSValue *old_stack;
	JSValue old_this;
	JSValue *results = NULL;
	slatejs_idx_t old_top;
	slatejs_idx_t old_cap;
	slatejs_idx_t i;
	slatejs_ret_t ret;

	if (base < 0) {
		return SLATEJS_EXEC_ERROR;
	}

	results = calloc(nrets > 0 ? nrets : 1, sizeof(*results));
	if (results == NULL) {
		return SLATEJS_EXEC_ERROR;
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
	slatejs_reserve_stack(ctx, nargs);
	for (i = 0; i < nargs; i++) {
		ctx->stack[ctx->top++] = old_stack[base + i];
		old_stack[base + i] = JS_UNDEFINED;
	}

	ret = func(ctx, udata);
	if (ret < 0) {
		JSValue err = slatejs_make_error(ctx, ctx->pending_error);
		slatejs_free_context_values(ctx);
		ctx->stack = old_stack;
		ctx->top = old_top;
		ctx->cap = old_cap;
		ctx->current_this = old_this;
		slatejs_push_owned(ctx, err);
		free(results);
		slatejs_drain_finalizers(ctx);
		return SLATEJS_EXEC_ERROR;
	}

	for (i = 0; i < nrets; i++) {
		if (ctx->top >= nrets - i) {
			results[i] = ctx->stack[ctx->top - nrets + i];
			ctx->stack[ctx->top - nrets + i] = JS_UNDEFINED;
		}
	}
	slatejs_free_context_values(ctx);
	ctx->stack = old_stack;
	ctx->top = old_top;
	ctx->cap = old_cap;
	ctx->current_this = old_this;

	for (i = 0; i < nrets; i++) {
		slatejs_push_owned(ctx, results[i]);
	}
	free(results);
	slatejs_drain_finalizers(ctx);
	return SLATEJS_EXEC_SUCCESS;
}

static char *slatejs_copy_nul_terminated(const char *src, slatejs_size_t len)
{
	char *copy = malloc(len + 1);

	if (copy == NULL) {
		return NULL;
	}
	memcpy(copy, src, len);
	copy[len] = '\0';
	return copy;
}

slatejs_int_t slatejs_pcompile_lstring_filename(slatejs_context *ctx, slatejs_uint_t flags,
		const char *src, slatejs_size_t len)
{
	const char *filename;
	char *source;
	JSValue compiled;
	void *old_opaque;

	if (ctx->top < 1 || src == NULL) {
		slatejs_push_string(ctx, "missing compile input");
		return SLATEJS_EXEC_ERROR;
	}

	while (len > 0 && src[len - 1] == '\0') {
		len--;
	}

	filename = slatejs_safe_to_string(ctx, -1);
	source = slatejs_copy_nul_terminated(src, len);
	if (source == NULL) {
		slatejs_pop(ctx);
		slatejs_push_string(ctx, "out of memory compiling JavaScript");
		return SLATEJS_EXEC_ERROR;
	}

	old_opaque = JS_GetContextOpaque(ctx->qctx);
	slatejs_sync_qjs_global(ctx);

	if (flags & SLATEJS_COMPILE_FUNCTION) {
		size_t wrap_len = len + 3;
		char *wrapped = malloc(wrap_len);
		if (wrapped == NULL) {
			JS_SetContextOpaque(ctx->qctx, old_opaque);
			free(source);
			slatejs_pop(ctx);
			slatejs_push_string(ctx, "out of memory compiling JavaScript");
			return SLATEJS_EXEC_ERROR;
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
	slatejs_pop(ctx);

	if (JS_IsException(compiled)) {
		slatejs_push_owned(ctx, JS_GetException(ctx->qctx));
		return SLATEJS_EXEC_ERROR;
	}

	slatejs_push_owned(ctx, compiled);
	return SLATEJS_EXEC_SUCCESS;
}

slatejs_int_t slatejs_pcompile(slatejs_context *ctx, slatejs_uint_t flags)
{
	const char *source;
	char *source_copy;
	slatejs_size_t source_len = 0;
	slatejs_idx_t base;
	JSValue result;
	slatejs_int_t ret;

	if (ctx->top < 2) {
		slatejs_push_string(ctx, "missing compile input");
		return SLATEJS_EXEC_ERROR;
	}

	base = ctx->top - 2;
	source = slatejs_safe_to_lstring(ctx, -2, &source_len);
	if (source == NULL) {
		slatejs_push_string(ctx, "missing compile source");
		return SLATEJS_EXEC_ERROR;
	}
	source_copy = slatejs_copy_nul_terminated(source, source_len);
	if (source_copy == NULL) {
		slatejs_push_string(ctx, "out of memory compiling JavaScript");
		return SLATEJS_EXEC_ERROR;
	}

	slatejs_dup(ctx, -1);
	ret = slatejs_pcompile_lstring_filename(ctx, flags, source_copy, source_len);
	free(source_copy);

	result = ctx->stack[ctx->top - 1];
	ctx->stack[ctx->top - 1] = JS_UNDEFINED;
	slatejs_free_range(ctx, base, ctx->top - base);
	ctx->top = base;
	slatejs_push_owned(ctx, result);
	return ret;
}

slatejs_int_t slatejs_get_type(slatejs_context *ctx, slatejs_idx_t idx)
{
	return slatejs_qjs_type_to_duk(slatejs_value_at(ctx, idx));
}

slatejs_bool_t slatejs_check_type(slatejs_context *ctx, slatejs_idx_t idx, slatejs_int_t type)
{
	return slatejs_get_type(ctx, idx) == type;
}

slatejs_bool_t slatejs_is_undefined(slatejs_context *ctx, slatejs_idx_t idx)
{
	return JS_IsUndefined(slatejs_value_at(ctx, idx));
}

slatejs_bool_t slatejs_is_null(slatejs_context *ctx, slatejs_idx_t idx)
{
	return JS_IsNull(slatejs_value_at(ctx, idx));
}

slatejs_bool_t slatejs_is_null_or_undefined(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue val = slatejs_value_at(ctx, idx);
	return JS_IsNull(val) || JS_IsUndefined(val);
}

slatejs_bool_t slatejs_is_boolean(slatejs_context *ctx, slatejs_idx_t idx)
{
	return JS_IsBool(slatejs_value_at(ctx, idx));
}

slatejs_bool_t slatejs_is_number(slatejs_context *ctx, slatejs_idx_t idx)
{
	return JS_IsNumber(slatejs_value_at(ctx, idx));
}

slatejs_bool_t slatejs_is_string(slatejs_context *ctx, slatejs_idx_t idx)
{
	return JS_IsString(slatejs_value_at(ctx, idx));
}

slatejs_bool_t slatejs_get_boolean(slatejs_context *ctx, slatejs_idx_t idx)
{
	int ret = JS_ToBool(ctx->qctx, slatejs_value_at(ctx, idx));
	return ret > 0;
}

slatejs_int_t slatejs_get_int(slatejs_context *ctx, slatejs_idx_t idx)
{
	int32_t ret = 0;
	JS_ToInt32(ctx->qctx, &ret, slatejs_value_at(ctx, idx));
	return ret;
}

slatejs_uint_t slatejs_get_uint(slatejs_context *ctx, slatejs_idx_t idx)
{
	uint32_t ret = 0;
	JS_ToUint32(ctx->qctx, &ret, slatejs_value_at(ctx, idx));
	return ret;
}

void *slatejs_get_pointer(slatejs_context *ctx, slatejs_idx_t idx)
{
	struct slatejs_pointer_ref *pref;

	pref = JS_GetOpaque(slatejs_value_at(ctx, idx), slatejs_pointer_class_id);
	return pref != NULL ? pref->ptr : NULL;
}

const char *slatejs_get_string(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue val = slatejs_value_at(ctx, idx);

	if (!JS_IsString(val)) {
		return NULL;
	}
	return slatejs_value_to_cached_string(ctx, val, NULL, false);
}

slatejs_size_t slatejs_get_length(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue val = slatejs_value_at(ctx, idx);

	if (JS_IsString(val)) {
		slatejs_size_t len = 0;
		const char *str = slatejs_value_to_cached_string(ctx, val, &len, true);
		(void)str;
		return len;
	}

	JSValue length = JS_GetPropertyStr(ctx->qctx, val, "length");
	uint32_t ret = 0;
	JS_ToUint32(ctx->qctx, &ret, length);
	JS_FreeValue(ctx->qctx, length);
	return ret;
}

slatejs_bool_t slatejs_to_boolean(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue *slot = slatejs_value_slot(ctx, idx);
	slatejs_bool_t ret;

	if (slot == NULL) {
		return false;
	}
	ret = JS_ToBool(ctx->qctx, *slot) > 0;
	JS_FreeValue(ctx->qctx, *slot);
	*slot = JS_NewBool(ctx->qctx, ret);
	return ret;
}

slatejs_int_t slatejs_to_int(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue *slot = slatejs_value_slot(ctx, idx);
	int32_t ret = 0;

	if (slot == NULL) {
		return 0;
	}
	JS_ToInt32(ctx->qctx, &ret, *slot);
	JS_FreeValue(ctx->qctx, *slot);
	*slot = JS_NewInt32(ctx->qctx, ret);
	return ret;
}

slatejs_uint_t slatejs_to_uint(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue *slot = slatejs_value_slot(ctx, idx);
	uint32_t ret = 0;

	if (slot == NULL) {
		return 0;
	}
	JS_ToUint32(ctx->qctx, &ret, *slot);
	JS_FreeValue(ctx->qctx, *slot);
	*slot = JS_NewUint32(ctx->qctx, ret);
	return ret;
}

uint32_t slatejs_to_uint32(slatejs_context *ctx, slatejs_idx_t idx)
{
	return slatejs_to_uint(ctx, idx);
}

const char *slatejs_to_string(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue *slot = slatejs_value_slot(ctx, idx);
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
	return slatejs_value_to_cached_string(ctx, *slot, NULL, true);
}

const char *slatejs_to_lstring(slatejs_context *ctx, slatejs_idx_t idx, slatejs_size_t *out_len)
{
	const char *ret = slatejs_to_string(ctx, idx);

	if (ret == NULL && out_len != NULL) {
		*out_len = 0;
		return NULL;
	}
	if (out_len != NULL) {
		*out_len = strlen(ret);
	}
	return ret;
}

slatejs_bool_t slatejs_require_boolean(slatejs_context *ctx, slatejs_idx_t idx)
{
	return slatejs_get_boolean(ctx, idx);
}

slatejs_int_t slatejs_require_int(slatejs_context *ctx, slatejs_idx_t idx)
{
	return slatejs_get_int(ctx, idx);
}

slatejs_double_t slatejs_require_number(slatejs_context *ctx, slatejs_idx_t idx)
{
	double ret = 0;
	JS_ToFloat64(ctx->qctx, &ret, slatejs_value_at(ctx, idx));
	return ret;
}

const char *slatejs_require_string(slatejs_context *ctx, slatejs_idx_t idx)
{
	return slatejs_safe_to_string(ctx, idx);
}

const char *slatejs_require_lstring(slatejs_context *ctx, slatejs_idx_t idx,
		slatejs_size_t *out_len)
{
	return slatejs_safe_to_lstring(ctx, idx, out_len);
}

slatejs_bool_t slatejs_opt_boolean(slatejs_context *ctx, slatejs_idx_t idx,
		slatejs_bool_t def_val)
{
	if (slatejs_is_undefined(ctx, idx)) {
		return def_val;
	}
	return slatejs_get_boolean(ctx, idx);
}

const char *slatejs_safe_to_string(slatejs_context *ctx, slatejs_idx_t idx)
{
	return slatejs_safe_to_lstring(ctx, idx, NULL);
}

const char *slatejs_safe_to_lstring(slatejs_context *ctx, slatejs_idx_t idx,
		slatejs_size_t *out_len)
{
	return slatejs_value_to_cached_string(ctx, slatejs_value_at(ctx, idx),
			out_len, true);
}

const char *slatejs_safe_to_stacktrace(slatejs_context *ctx, slatejs_idx_t idx)
{
	JSValue val = slatejs_value_at(ctx, idx);
	JSValue stack;
	const char *ret;

	if (JS_IsObject(val)) {
		stack = JS_GetPropertyStr(ctx->qctx, val, "stack");
		if (!JS_IsUndefined(stack)) {
			ret = slatejs_value_to_cached_string(ctx, stack, NULL, true);
			JS_FreeValue(ctx->qctx, stack);
			return ret;
		}
		JS_FreeValue(ctx->qctx, stack);
	}

	return slatejs_safe_to_string(ctx, idx);
}

slatejs_bool_t slatejs_strict_equals(slatejs_context *ctx, slatejs_idx_t idx1,
		slatejs_idx_t idx2)
{
	return JS_StrictEq(ctx->qctx, slatejs_value_at(ctx, idx1),
			slatejs_value_at(ctx, idx2));
}

void slatejs_concat(slatejs_context *ctx, slatejs_idx_t count)
{
	slatejs_idx_t start;
	slatejs_idx_t i;
	size_t total = 0;
	char *buf;
	size_t off = 0;

	if (count <= 0) {
		slatejs_push_string(ctx, "");
		return;
	}
	if (count > ctx->top) {
		count = ctx->top;
	}

	start = ctx->top - count;
	for (i = start; i < ctx->top; i++) {
		slatejs_size_t len = 0;
		(void)slatejs_value_to_cached_string(ctx, ctx->stack[i], &len, true);
		total += len;
	}

	buf = malloc(total + 1);
	if (buf == NULL) {
		slatejs_pop_n(ctx, count);
		slatejs_push_string(ctx, "");
		return;
	}

	for (i = start; i < ctx->top; i++) {
		slatejs_size_t len = 0;
		const char *part = slatejs_value_to_cached_string(ctx, ctx->stack[i],
				&len, true);
		memcpy(buf + off, part, len);
		off += len;
	}
	buf[off] = '\0';
	slatejs_pop_n(ctx, count);
	slatejs_push_lstring(ctx, buf, off);
	free(buf);
}

slatejs_ret_t slatejs_error(slatejs_context *ctx, slatejs_errcode_t err_code,
		const char *fmt, ...)
{
	va_list ap;

	(void)err_code;

	va_start(ap, fmt);
	slatejs_set_pending_errorv(ctx, fmt, ap);
	va_end(ap);
	return SLATEJS_RET_TYPE_ERROR;
}

slatejs_ret_t slatejs_generic_error(slatejs_context *ctx, const char *fmt, ...)
{
	va_list ap;

	va_start(ap, fmt);
	slatejs_set_pending_errorv(ctx, fmt, ap);
	va_end(ap);
	return SLATEJS_RET_TYPE_ERROR;
}

static void slatejs_object_finalizer(JSRuntime *rt, JSValue val)
{
	struct slatejs_object_meta *meta = JS_GetOpaque(val, slatejs_object_class_id);

	(void)rt;
	/*
	 * QuickJS calls this after object properties are released. The generated
	 * destructor still expects QuickJS-style private lookup, so queue it and
	 * provide the cached native private pointer from a safe shim frame.
	 */
	slatejs_queue_finalizer(meta);
}

static void slatejs_cfunc_finalizer(JSRuntime *rt, JSValue val)
{
	struct slatejs_cfunc_ref *ref = JS_GetOpaque(val, slatejs_cfunc_class_id);

	(void)rt;
	free(ref);
}

static void slatejs_pointer_finalizer(JSRuntime *rt, JSValue val)
{
	struct slatejs_pointer_ref *pref = JS_GetOpaque(val, slatejs_pointer_class_id);

	(void)rt;
	free(pref);
}
