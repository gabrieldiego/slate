/*
 * Slate stack-style JavaScript binding API backed by QuickJS.
 */

#ifndef SLATE_QUICKJS_API_H
#define SLATE_QUICKJS_API_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <stdint.h>

typedef struct slatejs_context slatejs_context;

typedef int slatejs_ret_t;
typedef int slatejs_bool_t;
typedef int slatejs_int_t;
typedef unsigned int slatejs_uint_t;
typedef uint32_t slatejs_uarridx_t;
typedef uint32_t slatejs_arridx_t;
typedef int slatejs_idx_t;
typedef size_t slatejs_size_t;
typedef double slatejs_double_t;
typedef int slatejs_errcode_t;

typedef slatejs_ret_t (*slatejs_c_function)(slatejs_context *ctx);
typedef slatejs_ret_t (*slatejs_safe_call_function)(slatejs_context *ctx, void *udata);

typedef void *(*slatejs_alloc_function)(void *udata, slatejs_size_t size);
typedef void *(*slatejs_realloc_function)(void *udata, void *ptr, slatejs_size_t size);
typedef void (*slatejs_free_function)(void *udata, void *ptr);
typedef void (*slatejs_fatal_function)(void *udata, const char *msg);

typedef struct {
	slatejs_alloc_function alloc_func;
	slatejs_realloc_function realloc_func;
	slatejs_free_function free_func;
	void *udata;
} slatejs_memory_functions;

#define SLATEJS_VARARGS (-1)

#define SLATEJS_EXEC_SUCCESS 0
#define SLATEJS_EXEC_ERROR 1

#define SLATEJS_ERR_NONE 0
#define SLATEJS_ERR_ERROR 1
#define SLATEJS_RET_TYPE_ERROR (-6)

#define SLATEJS_TYPE_NONE 0
#define SLATEJS_TYPE_UNDEFINED 1
#define SLATEJS_TYPE_NULL 2
#define SLATEJS_TYPE_BOOLEAN 3
#define SLATEJS_TYPE_NUMBER 4
#define SLATEJS_TYPE_STRING 5
#define SLATEJS_TYPE_OBJECT 6
#define SLATEJS_TYPE_POINTER 7

#define SLATEJS_COMPILE_EVAL (1u << 0)
#define SLATEJS_COMPILE_FUNCTION (1u << 1)

#define SLATEJS_GC_COMPACT (1u << 0)

#define SLATEJS_DEFPROP_HAVE_VALUE (1u << 0)
#define SLATEJS_DEFPROP_HAVE_WRITABLE (1u << 1)
#define SLATEJS_DEFPROP_WRITABLE (1u << 2)
#define SLATEJS_DEFPROP_HAVE_ENUMERABLE (1u << 3)
#define SLATEJS_DEFPROP_ENUMERABLE (1u << 4)
#define SLATEJS_DEFPROP_HAVE_CONFIGURABLE (1u << 5)
#define SLATEJS_DEFPROP_CONFIGURABLE (1u << 6)
#define SLATEJS_DEFPROP_HAVE_GETTER (1u << 7)
#define SLATEJS_DEFPROP_HAVE_SETTER (1u << 8)

#define SLATEJS_BUFOBJ_UINT8CLAMPEDARRAY 0

slatejs_context *slatejs_create_heap(slatejs_alloc_function alloc_func,
		slatejs_realloc_function realloc_func,
		slatejs_free_function free_func,
		void *heap_udata,
		slatejs_fatal_function fatal_handler);
void slatejs_destroy_heap(slatejs_context *ctx);
void slatejs_gc(slatejs_context *ctx, slatejs_uint_t flags);
slatejs_int_t slatejs_execute_pending_jobs(slatejs_context *ctx,
		slatejs_uint_t max_jobs);
void slatejs_get_memory_functions(slatejs_context *ctx, slatejs_memory_functions *out_funcs);

slatejs_idx_t slatejs_get_top(slatejs_context *ctx);
void slatejs_set_top(slatejs_context *ctx, slatejs_idx_t top);
slatejs_idx_t slatejs_normalize_index(slatejs_context *ctx, slatejs_idx_t idx);

void slatejs_pop(slatejs_context *ctx);
void slatejs_pop_2(slatejs_context *ctx);
void slatejs_pop_3(slatejs_context *ctx);
void slatejs_pop_n(slatejs_context *ctx, slatejs_idx_t count);

void slatejs_dup(slatejs_context *ctx, slatejs_idx_t idx);
void slatejs_dup_top(slatejs_context *ctx);
void slatejs_insert(slatejs_context *ctx, slatejs_idx_t idx);
void slatejs_remove(slatejs_context *ctx, slatejs_idx_t idx);
void slatejs_replace(slatejs_context *ctx, slatejs_idx_t idx);

void slatejs_push_undefined(slatejs_context *ctx);
void slatejs_push_null(slatejs_context *ctx);
void slatejs_push_boolean(slatejs_context *ctx, slatejs_bool_t val);
void slatejs_push_int(slatejs_context *ctx, slatejs_int_t val);
void slatejs_push_uint(slatejs_context *ctx, slatejs_uint_t val);
void slatejs_push_number(slatejs_context *ctx, slatejs_double_t val);
void slatejs_push_string(slatejs_context *ctx, const char *str);
void slatejs_push_lstring(slatejs_context *ctx, const char *str, slatejs_size_t len);
void slatejs_push_pointer(slatejs_context *ctx, void *ptr);
slatejs_idx_t slatejs_push_object(slatejs_context *ctx);
slatejs_idx_t slatejs_push_array(slatejs_context *ctx);
void *slatejs_push_buffer(slatejs_context *ctx, slatejs_size_t size, slatejs_bool_t dynamic);
void slatejs_push_buffer_object(slatejs_context *ctx, slatejs_idx_t idx,
		slatejs_size_t byte_offset, slatejs_size_t byte_length,
		slatejs_uint_t flags);
slatejs_idx_t slatejs_push_c_function(slatejs_context *ctx, slatejs_c_function func,
		slatejs_idx_t nargs);
void slatejs_push_global_object(slatejs_context *ctx);
void slatejs_push_this(slatejs_context *ctx);
void slatejs_push_thread(slatejs_context *ctx);
slatejs_idx_t slatejs_push_error_object(slatejs_context *ctx, slatejs_errcode_t err_code,
		const char *fmt, ...);
void slatejs_push_sprintf(slatejs_context *ctx, const char *fmt, ...);
void slatejs_push_context_dump(slatejs_context *ctx);

slatejs_context *slatejs_require_context(slatejs_context *ctx, slatejs_idx_t idx);

slatejs_bool_t slatejs_get_prop(slatejs_context *ctx, slatejs_idx_t obj_idx);
slatejs_bool_t slatejs_get_prop_string(slatejs_context *ctx, slatejs_idx_t obj_idx,
		const char *key);
slatejs_bool_t slatejs_get_prop_index(slatejs_context *ctx, slatejs_idx_t obj_idx,
		slatejs_uarridx_t idx);
slatejs_bool_t slatejs_get_global_string(slatejs_context *ctx, const char *key);
slatejs_bool_t slatejs_put_prop(slatejs_context *ctx, slatejs_idx_t obj_idx);
slatejs_bool_t slatejs_put_prop_string(slatejs_context *ctx, slatejs_idx_t obj_idx,
		const char *key);
slatejs_bool_t slatejs_put_prop_index(slatejs_context *ctx, slatejs_idx_t obj_idx,
		slatejs_uarridx_t idx);
slatejs_bool_t slatejs_put_global_string(slatejs_context *ctx, const char *key);
slatejs_bool_t slatejs_del_prop(slatejs_context *ctx, slatejs_idx_t obj_idx);
slatejs_bool_t slatejs_del_prop_string(slatejs_context *ctx, slatejs_idx_t obj_idx,
		const char *key);
slatejs_bool_t slatejs_del_prop_index(slatejs_context *ctx, slatejs_idx_t obj_idx,
		slatejs_uarridx_t idx);
slatejs_bool_t slatejs_has_prop(slatejs_context *ctx, slatejs_idx_t obj_idx);
void slatejs_def_prop(slatejs_context *ctx, slatejs_idx_t obj_idx, slatejs_uint_t flags);
void slatejs_set_prototype(slatejs_context *ctx, slatejs_idx_t idx);
void slatejs_get_prototype(slatejs_context *ctx, slatejs_idx_t idx);
void slatejs_set_finalizer(slatejs_context *ctx, slatejs_idx_t idx);
void slatejs_set_global_object(slatejs_context *ctx);

slatejs_int_t slatejs_pcompile(slatejs_context *ctx, slatejs_uint_t flags);
slatejs_int_t slatejs_pcompile_lstring_filename(slatejs_context *ctx, slatejs_uint_t flags,
		const char *src, slatejs_size_t len);
void slatejs_call(slatejs_context *ctx, slatejs_idx_t nargs);
slatejs_int_t slatejs_pcall(slatejs_context *ctx, slatejs_idx_t nargs);
slatejs_int_t slatejs_pcall_method(slatejs_context *ctx, slatejs_idx_t nargs);
slatejs_int_t slatejs_safe_call(slatejs_context *ctx, slatejs_safe_call_function func,
		void *udata, slatejs_idx_t nargs, slatejs_idx_t nrets);

slatejs_int_t slatejs_get_type(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_bool_t slatejs_check_type(slatejs_context *ctx, slatejs_idx_t idx, slatejs_int_t type);
slatejs_bool_t slatejs_is_undefined(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_bool_t slatejs_is_null(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_bool_t slatejs_is_null_or_undefined(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_bool_t slatejs_is_boolean(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_bool_t slatejs_is_number(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_bool_t slatejs_is_string(slatejs_context *ctx, slatejs_idx_t idx);

slatejs_bool_t slatejs_get_boolean(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_int_t slatejs_get_int(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_uint_t slatejs_get_uint(slatejs_context *ctx, slatejs_idx_t idx);
void *slatejs_get_pointer(slatejs_context *ctx, slatejs_idx_t idx);
const char *slatejs_get_string(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_size_t slatejs_get_length(slatejs_context *ctx, slatejs_idx_t idx);

slatejs_bool_t slatejs_to_boolean(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_int_t slatejs_to_int(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_uint_t slatejs_to_uint(slatejs_context *ctx, slatejs_idx_t idx);
uint32_t slatejs_to_uint32(slatejs_context *ctx, slatejs_idx_t idx);
const char *slatejs_to_string(slatejs_context *ctx, slatejs_idx_t idx);
const char *slatejs_to_lstring(slatejs_context *ctx, slatejs_idx_t idx, slatejs_size_t *out_len);

slatejs_bool_t slatejs_require_boolean(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_int_t slatejs_require_int(slatejs_context *ctx, slatejs_idx_t idx);
slatejs_double_t slatejs_require_number(slatejs_context *ctx, slatejs_idx_t idx);
const char *slatejs_require_string(slatejs_context *ctx, slatejs_idx_t idx);
const char *slatejs_require_lstring(slatejs_context *ctx, slatejs_idx_t idx,
		slatejs_size_t *out_len);
slatejs_bool_t slatejs_opt_boolean(slatejs_context *ctx, slatejs_idx_t idx,
		slatejs_bool_t def_val);

const char *slatejs_safe_to_string(slatejs_context *ctx, slatejs_idx_t idx);
const char *slatejs_safe_to_lstring(slatejs_context *ctx, slatejs_idx_t idx,
		slatejs_size_t *out_len);
const char *slatejs_safe_to_stacktrace(slatejs_context *ctx, slatejs_idx_t idx);

slatejs_bool_t slatejs_strict_equals(slatejs_context *ctx, slatejs_idx_t idx1,
		slatejs_idx_t idx2);
void slatejs_concat(slatejs_context *ctx, slatejs_idx_t count);

slatejs_ret_t slatejs_error(slatejs_context *ctx, slatejs_errcode_t err_code,
		const char *fmt, ...);
slatejs_ret_t slatejs_generic_error(slatejs_context *ctx, const char *fmt, ...);

#endif
