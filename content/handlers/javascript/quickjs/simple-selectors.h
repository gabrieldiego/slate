/*
 * Small selector helpers for the QuickJS DOM bindings.
 *
 * This is intentionally not a full selector engine. It covers the cheap cases
 * used heavily by libraries for feature detection and simple lookups.
 */

#ifndef SLATE_QUICKJS_SIMPLE_SELECTORS_H
#define SLATE_QUICKJS_SIMPLE_SELECTORS_H

#include <stdbool.h>
#include <ctype.h>
#include <string.h>
#include <strings.h>

#include <dom/dom.h>

#include "utils/corestrings.h"
#include "javascript/quickjs/api.h"

#if defined(__GNUC__)
#define SLATE_SELECTOR_UNUSED __attribute__((unused))
#else
#define SLATE_SELECTOR_UNUSED
#endif

static const char *
slatejs_selector_trim(const char *s, size_t *len)
{
	while (*len > 0 && isspace((unsigned char)*s)) {
		s++;
		(*len)--;
	}
	while (*len > 0 && isspace((unsigned char)s[*len - 1])) {
		(*len)--;
	}
	return s;
}

static bool
slatejs_selector_attr_equals(dom_element *element, dom_string *attr,
		const char *value, size_t value_len)
{
	dom_string *attr_value = NULL;
	bool ret = false;

	if (attr == NULL || value == NULL) {
		return true;
	}

	if (dom_element_get_attribute(element, attr, &attr_value) != DOM_NO_ERR ||
			attr_value == NULL) {
		return false;
	}

	ret = dom_string_length(attr_value) == value_len &&
			strncmp(dom_string_data(attr_value), value, value_len) == 0;
	dom_string_unref(attr_value);
	return ret;
}

static bool
slatejs_selector_class_contains(dom_element *element,
		const char *value, size_t value_len)
{
	dom_string *class_value = NULL;
	const char *data;
	size_t len;
	size_t pos = 0;

	if (value == NULL) {
		return true;
	}

	if (dom_element_get_attribute(element, corestring_dom_class,
			&class_value) != DOM_NO_ERR || class_value == NULL) {
		return false;
	}

	data = dom_string_data(class_value);
	len = dom_string_length(class_value);
	while (pos < len) {
		size_t start;
		size_t end;

		while (pos < len && isspace((unsigned char)data[pos])) {
			pos++;
		}
		start = pos;
		while (pos < len && !isspace((unsigned char)data[pos])) {
			pos++;
		}
		end = pos;
		if (end - start == value_len &&
				strncmp(data + start, value, value_len) == 0) {
			dom_string_unref(class_value);
			return true;
		}
	}

	dom_string_unref(class_value);
	return false;
}

static bool
slatejs_selector_part_matches(dom_node *node, const char *selector,
		size_t selector_len)
{
	dom_node_type node_type;
	dom_string *node_name = NULL;
	const char *tag = NULL;
	size_t tag_len = 0;
	const char *id = NULL;
	size_t id_len = 0;
	const char *cls = NULL;
	size_t cls_len = 0;
	size_t pos = 0;
	size_t start;
	bool ret;

	selector = slatejs_selector_trim(selector, &selector_len);
	if (selector_len == 0) {
		return false;
	}

	for (pos = 0; pos < selector_len; pos++) {
		switch (selector[pos]) {
		case ' ':
		case '\t':
		case '\n':
		case '\r':
		case '>':
		case '+':
		case '~':
		case '[':
		case ':':
			return false;
		default:
			break;
		}
	}

	if (dom_node_get_node_type(node, &node_type) != DOM_NO_ERR ||
			node_type != DOM_ELEMENT_NODE) {
		return false;
	}

	pos = 0;
	start = pos;
	while (pos < selector_len && selector[pos] != '#' &&
			selector[pos] != '.') {
		pos++;
	}
	tag = selector + start;
	tag_len = pos - start;

	while (pos < selector_len) {
		char marker = selector[pos++];
		start = pos;
		while (pos < selector_len && selector[pos] != '#' &&
				selector[pos] != '.') {
			pos++;
		}
		if (marker == '#') {
			id = selector + start;
			id_len = pos - start;
		} else if (marker == '.') {
			cls = selector + start;
			cls_len = pos - start;
		}
	}

	if (tag_len == 0 && id == NULL && cls == NULL) {
		return false;
	}

	if (tag_len > 0 && !(tag_len == 1 && tag[0] == '*')) {
		if (dom_node_get_node_name(node, &node_name) != DOM_NO_ERR ||
				node_name == NULL) {
			return false;
		}
		ret = dom_string_length(node_name) == tag_len &&
				strncasecmp(dom_string_data(node_name), tag, tag_len) == 0;
		dom_string_unref(node_name);
		if (!ret) {
			return false;
		}
	}

	if (!slatejs_selector_attr_equals((dom_element *)node,
			corestring_dom_id, id, id_len)) {
		return false;
	}

	if (!slatejs_selector_class_contains((dom_element *)node, cls, cls_len)) {
		return false;
	}

	return true;
}

static bool
slatejs_selector_prev_token(const char *start, const char **endp,
		const char **token, size_t *token_len)
{
	const char *end = *endp;
	const char *begin;

	while (end > start && isspace((unsigned char)end[-1])) {
		end--;
	}
	if (end == start) {
		return false;
	}

	begin = end;
	while (begin > start && !isspace((unsigned char)begin[-1])) {
		begin--;
	}

	*token = begin;
	*token_len = (size_t)(end - begin);
	*endp = begin;
	return true;
}

static bool
slatejs_selector_sequence_matches(dom_node *node, const char *selector,
		size_t selector_len)
{
	const char *start;
	const char *scan_end;
	const char *token;
	size_t token_len;
	dom_node *cur = node;
	bool cur_owned = false;

	start = slatejs_selector_trim(selector, &selector_len);
	scan_end = start + selector_len;

	if (!slatejs_selector_prev_token(start, &scan_end, &token, &token_len)) {
		return false;
	}

	if (!slatejs_selector_part_matches(cur, token, token_len)) {
		return false;
	}

	while (slatejs_selector_prev_token(start, &scan_end,
			&token, &token_len)) {
		dom_node *candidate = NULL;
		bool found = false;

		if (dom_node_get_parent_node(cur, &candidate) != DOM_NO_ERR) {
			if (cur_owned) {
				dom_node_unref(cur);
			}
			return false;
		}
		if (cur_owned) {
			dom_node_unref(cur);
			cur_owned = false;
		}

		while (candidate != NULL) {
			dom_node *parent = NULL;

			if (slatejs_selector_part_matches(candidate,
					token, token_len)) {
				cur = candidate;
				cur_owned = true;
				found = true;
				break;
			}

			if (dom_node_get_parent_node(candidate, &parent) !=
					DOM_NO_ERR) {
				dom_node_unref(candidate);
				break;
			}
			dom_node_unref(candidate);
			candidate = parent;
		}

		if (!found) {
			return false;
		}
	}

	if (cur_owned) {
		dom_node_unref(cur);
	}
	return true;
}

static bool
slatejs_selector_matches(dom_node *node, const char *selector,
		size_t selector_len)
{
	const char *part = selector;
	size_t part_len = 0;
	size_t pos;

	for (pos = 0; pos <= selector_len; pos++) {
		if (pos == selector_len || selector[pos] == ',') {
			if (slatejs_selector_sequence_matches(node,
					part, part_len)) {
				return true;
			}
			if (pos < selector_len) {
				part = selector + pos + 1;
				part_len = 0;
			}
		} else {
			part_len++;
		}
	}

	return false;
}

static slatejs_ret_t
slatejs_selector_push_all(slatejs_context *ctx, dom_node *root,
		const char *selector, size_t selector_len, bool first_only)
{
	dom_node_type root_type;
	dom_string *star = NULL;
	dom_nodelist *nodes = NULL;
	uint32_t length = 0;
	uint32_t out_idx = 0;
	uint32_t idx;
	slatejs_idx_t array_idx = -1;

	if (dom_node_get_node_type(root, &root_type) != DOM_NO_ERR) {
		return 0;
	}

	if (dom_string_create((const uint8_t *)"*", 1, &star) != DOM_NO_ERR) {
		return 0;
	}

	if (root_type == DOM_DOCUMENT_NODE) {
		if (dom_document_get_elements_by_tag_name((dom_document *)root,
				star, &nodes) != DOM_NO_ERR) {
			dom_string_unref(star);
			return 0;
		}
	} else if (root_type == DOM_ELEMENT_NODE) {
		if (dom_element_get_elements_by_tag_name((dom_element *)root,
				star, &nodes) != DOM_NO_ERR) {
			dom_string_unref(star);
			return 0;
		}
	} else {
		dom_string_unref(star);
		if (first_only) {
			slatejs_push_null(ctx);
		} else {
			slatejs_push_array(ctx);
		}
		return 1;
	}
	dom_string_unref(star);

	if (dom_nodelist_get_length(nodes, &length) != DOM_NO_ERR) {
		dom_nodelist_unref(nodes);
		return 0;
	}

	if (!first_only) {
		array_idx = slatejs_push_array(ctx);
	}

	for (idx = 0; idx < length; idx++) {
		dom_node *node = NULL;

		if (dom_nodelist_item(nodes, idx, &node) != DOM_NO_ERR ||
				node == NULL) {
			continue;
		}

		if (slatejs_selector_matches(node, selector, selector_len)) {
			if (first_only) {
				slatejs_bind_push_node(ctx, node);
				dom_node_unref(node);
				dom_nodelist_unref(nodes);
				return 1;
			}

			if (slatejs_bind_push_node(ctx, node)) {
				slatejs_put_prop_index(ctx, array_idx, out_idx++);
			}
		}
		dom_node_unref(node);
	}

	dom_nodelist_unref(nodes);
	if (first_only) {
		slatejs_push_null(ctx);
	}
	return 1;
}

static slatejs_ret_t SLATE_SELECTOR_UNUSED
slatejs_selector_push_closest(slatejs_context *ctx, dom_node *node,
		const char *selector, size_t selector_len)
{
	dom_node *cur = dom_node_ref(node);

	while (cur != NULL) {
		dom_node *parent = NULL;

		if (slatejs_selector_matches(cur, selector, selector_len)) {
			slatejs_bind_push_node(ctx, cur);
			dom_node_unref(cur);
			return 1;
		}

		if (dom_node_get_parent_node(cur, &parent) != DOM_NO_ERR) {
			dom_node_unref(cur);
			break;
		}
		dom_node_unref(cur);
		cur = parent;
	}

	slatejs_push_null(ctx);
	return 1;
}

#undef SLATE_SELECTOR_UNUSED

#endif
