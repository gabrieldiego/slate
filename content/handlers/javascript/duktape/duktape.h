/*
 * QuickJS-backed subset of the Duktape API used by Slate's generated DOM
 * bindings. This keeps the current slategenbind output usable while the engine
 * underneath is QuickJS.
 */

#ifndef SLATE_QJS_DUKTAPE_H
#define SLATE_QJS_DUKTAPE_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <stdint.h>

typedef struct duk_context duk_context;

typedef int duk_ret_t;
typedef int duk_bool_t;
typedef int duk_int_t;
typedef unsigned int duk_uint_t;
typedef uint32_t duk_uarridx_t;
typedef uint32_t duk_arridx_t;
typedef int duk_idx_t;
typedef size_t duk_size_t;
typedef double duk_double_t;
typedef int duk_errcode_t;

typedef duk_ret_t (*duk_c_function)(duk_context *ctx);
typedef duk_ret_t (*duk_safe_call_function)(duk_context *ctx, void *udata);

typedef void *(*duk_alloc_function)(void *udata, duk_size_t size);
typedef void *(*duk_realloc_function)(void *udata, void *ptr, duk_size_t size);
typedef void (*duk_free_function)(void *udata, void *ptr);
typedef void (*duk_fatal_function)(void *udata, const char *msg);

typedef struct {
	duk_alloc_function alloc_func;
	duk_realloc_function realloc_func;
	duk_free_function free_func;
	void *udata;
} duk_memory_functions;

#define DUK_VARARGS (-1)

#define DUK_EXEC_SUCCESS 0
#define DUK_EXEC_ERROR 1

#define DUK_ERR_NONE 0
#define DUK_ERR_ERROR 1
#define DUK_RET_TYPE_ERROR (-6)

#define DUK_TYPE_NONE 0
#define DUK_TYPE_UNDEFINED 1
#define DUK_TYPE_NULL 2
#define DUK_TYPE_BOOLEAN 3
#define DUK_TYPE_NUMBER 4
#define DUK_TYPE_STRING 5
#define DUK_TYPE_OBJECT 6
#define DUK_TYPE_POINTER 7

#define DUK_COMPILE_EVAL (1u << 0)
#define DUK_COMPILE_FUNCTION (1u << 1)

#define DUK_GC_COMPACT (1u << 0)

#define DUK_DEFPROP_HAVE_VALUE (1u << 0)
#define DUK_DEFPROP_HAVE_WRITABLE (1u << 1)
#define DUK_DEFPROP_WRITABLE (1u << 2)
#define DUK_DEFPROP_HAVE_ENUMERABLE (1u << 3)
#define DUK_DEFPROP_ENUMERABLE (1u << 4)
#define DUK_DEFPROP_HAVE_CONFIGURABLE (1u << 5)
#define DUK_DEFPROP_CONFIGURABLE (1u << 6)
#define DUK_DEFPROP_HAVE_GETTER (1u << 7)
#define DUK_DEFPROP_HAVE_SETTER (1u << 8)

#define DUK_BUFOBJ_UINT8CLAMPEDARRAY 0

duk_context *duk_create_heap(duk_alloc_function alloc_func,
		duk_realloc_function realloc_func,
		duk_free_function free_func,
		void *heap_udata,
		duk_fatal_function fatal_handler);
void duk_destroy_heap(duk_context *ctx);
void duk_gc(duk_context *ctx, duk_uint_t flags);
void duk_get_memory_functions(duk_context *ctx, duk_memory_functions *out_funcs);

duk_idx_t duk_get_top(duk_context *ctx);
void duk_set_top(duk_context *ctx, duk_idx_t top);
duk_idx_t duk_normalize_index(duk_context *ctx, duk_idx_t idx);

void duk_pop(duk_context *ctx);
void duk_pop_2(duk_context *ctx);
void duk_pop_3(duk_context *ctx);
void duk_pop_n(duk_context *ctx, duk_idx_t count);

void duk_dup(duk_context *ctx, duk_idx_t idx);
void duk_dup_top(duk_context *ctx);
void duk_insert(duk_context *ctx, duk_idx_t idx);
void duk_remove(duk_context *ctx, duk_idx_t idx);
void duk_replace(duk_context *ctx, duk_idx_t idx);

void duk_push_undefined(duk_context *ctx);
void duk_push_null(duk_context *ctx);
void duk_push_boolean(duk_context *ctx, duk_bool_t val);
void duk_push_int(duk_context *ctx, duk_int_t val);
void duk_push_uint(duk_context *ctx, duk_uint_t val);
void duk_push_number(duk_context *ctx, duk_double_t val);
void duk_push_string(duk_context *ctx, const char *str);
void duk_push_lstring(duk_context *ctx, const char *str, duk_size_t len);
void duk_push_pointer(duk_context *ctx, void *ptr);
duk_idx_t duk_push_object(duk_context *ctx);
duk_idx_t duk_push_array(duk_context *ctx);
void *duk_push_buffer(duk_context *ctx, duk_size_t size, duk_bool_t dynamic);
void duk_push_buffer_object(duk_context *ctx, duk_idx_t idx,
		duk_size_t byte_offset, duk_size_t byte_length,
		duk_uint_t flags);
duk_idx_t duk_push_c_function(duk_context *ctx, duk_c_function func,
		duk_idx_t nargs);
void duk_push_global_object(duk_context *ctx);
void duk_push_this(duk_context *ctx);
void duk_push_thread(duk_context *ctx);
duk_idx_t duk_push_error_object(duk_context *ctx, duk_errcode_t err_code,
		const char *fmt, ...);
void duk_push_sprintf(duk_context *ctx, const char *fmt, ...);
void duk_push_context_dump(duk_context *ctx);

duk_context *duk_require_context(duk_context *ctx, duk_idx_t idx);

duk_bool_t duk_get_prop(duk_context *ctx, duk_idx_t obj_idx);
duk_bool_t duk_get_prop_string(duk_context *ctx, duk_idx_t obj_idx,
		const char *key);
duk_bool_t duk_get_prop_index(duk_context *ctx, duk_idx_t obj_idx,
		duk_uarridx_t idx);
duk_bool_t duk_get_global_string(duk_context *ctx, const char *key);
duk_bool_t duk_put_prop(duk_context *ctx, duk_idx_t obj_idx);
duk_bool_t duk_put_prop_string(duk_context *ctx, duk_idx_t obj_idx,
		const char *key);
duk_bool_t duk_put_prop_index(duk_context *ctx, duk_idx_t obj_idx,
		duk_uarridx_t idx);
duk_bool_t duk_put_global_string(duk_context *ctx, const char *key);
duk_bool_t duk_del_prop(duk_context *ctx, duk_idx_t obj_idx);
duk_bool_t duk_del_prop_string(duk_context *ctx, duk_idx_t obj_idx,
		const char *key);
duk_bool_t duk_del_prop_index(duk_context *ctx, duk_idx_t obj_idx,
		duk_uarridx_t idx);
duk_bool_t duk_has_prop(duk_context *ctx, duk_idx_t obj_idx);
void duk_def_prop(duk_context *ctx, duk_idx_t obj_idx, duk_uint_t flags);
void duk_set_prototype(duk_context *ctx, duk_idx_t idx);
void duk_get_prototype(duk_context *ctx, duk_idx_t idx);
void duk_set_finalizer(duk_context *ctx, duk_idx_t idx);
void duk_set_global_object(duk_context *ctx);

duk_int_t duk_pcompile(duk_context *ctx, duk_uint_t flags);
duk_int_t duk_pcompile_lstring_filename(duk_context *ctx, duk_uint_t flags,
		const char *src, duk_size_t len);
void duk_call(duk_context *ctx, duk_idx_t nargs);
duk_int_t duk_pcall(duk_context *ctx, duk_idx_t nargs);
duk_int_t duk_pcall_method(duk_context *ctx, duk_idx_t nargs);
duk_int_t duk_safe_call(duk_context *ctx, duk_safe_call_function func,
		void *udata, duk_idx_t nargs, duk_idx_t nrets);

duk_int_t duk_get_type(duk_context *ctx, duk_idx_t idx);
duk_bool_t duk_check_type(duk_context *ctx, duk_idx_t idx, duk_int_t type);
duk_bool_t duk_is_undefined(duk_context *ctx, duk_idx_t idx);
duk_bool_t duk_is_null(duk_context *ctx, duk_idx_t idx);
duk_bool_t duk_is_null_or_undefined(duk_context *ctx, duk_idx_t idx);
duk_bool_t duk_is_boolean(duk_context *ctx, duk_idx_t idx);
duk_bool_t duk_is_number(duk_context *ctx, duk_idx_t idx);
duk_bool_t duk_is_string(duk_context *ctx, duk_idx_t idx);

duk_bool_t duk_get_boolean(duk_context *ctx, duk_idx_t idx);
duk_int_t duk_get_int(duk_context *ctx, duk_idx_t idx);
duk_uint_t duk_get_uint(duk_context *ctx, duk_idx_t idx);
void *duk_get_pointer(duk_context *ctx, duk_idx_t idx);
const char *duk_get_string(duk_context *ctx, duk_idx_t idx);
duk_size_t duk_get_length(duk_context *ctx, duk_idx_t idx);

duk_bool_t duk_to_boolean(duk_context *ctx, duk_idx_t idx);
duk_int_t duk_to_int(duk_context *ctx, duk_idx_t idx);
duk_uint_t duk_to_uint(duk_context *ctx, duk_idx_t idx);
uint32_t duk_to_uint32(duk_context *ctx, duk_idx_t idx);
const char *duk_to_string(duk_context *ctx, duk_idx_t idx);
const char *duk_to_lstring(duk_context *ctx, duk_idx_t idx, duk_size_t *out_len);

duk_bool_t duk_require_boolean(duk_context *ctx, duk_idx_t idx);
duk_int_t duk_require_int(duk_context *ctx, duk_idx_t idx);
duk_double_t duk_require_number(duk_context *ctx, duk_idx_t idx);
const char *duk_require_string(duk_context *ctx, duk_idx_t idx);
const char *duk_require_lstring(duk_context *ctx, duk_idx_t idx,
		duk_size_t *out_len);
duk_bool_t duk_opt_boolean(duk_context *ctx, duk_idx_t idx,
		duk_bool_t def_val);

const char *duk_safe_to_string(duk_context *ctx, duk_idx_t idx);
const char *duk_safe_to_lstring(duk_context *ctx, duk_idx_t idx,
		duk_size_t *out_len);
const char *duk_safe_to_stacktrace(duk_context *ctx, duk_idx_t idx);

duk_bool_t duk_strict_equals(duk_context *ctx, duk_idx_t idx1,
		duk_idx_t idx2);
void duk_concat(duk_context *ctx, duk_idx_t count);

duk_ret_t duk_error(duk_context *ctx, duk_errcode_t err_code,
		const char *fmt, ...);
duk_ret_t duk_generic_error(duk_context *ctx, const char *fmt, ...);

#endif
