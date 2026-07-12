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
#include <stdlib.h>
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

enum slatejs_selector_attr_operator {
	SLATEJS_SELECTOR_ATTR_EXISTS,
	SLATEJS_SELECTOR_ATTR_EQUALS,
	SLATEJS_SELECTOR_ATTR_PREFIX,
	SLATEJS_SELECTOR_ATTR_SUFFIX,
	SLATEJS_SELECTOR_ATTR_CONTAINS,
	SLATEJS_SELECTOR_ATTR_WORD,
	SLATEJS_SELECTOR_ATTR_DASH
};

struct slatejs_selector_attr {
	const char *name;
	size_t name_len;
	const char *value;
	size_t value_len;
	enum slatejs_selector_attr_operator op;
};

static bool
slatejs_selector_bytes_equal(const char *a, size_t a_len,
		const char *b, size_t b_len)
{
	return a_len == b_len && strncmp(a, b, a_len) == 0;
}

static bool
slatejs_selector_bytes_contains(const char *data, size_t data_len,
		const char *needle, size_t needle_len)
{
	size_t pos;

	if (needle_len == 0) {
		return true;
	}
	if (needle_len > data_len) {
		return false;
	}
	for (pos = 0; pos + needle_len <= data_len; pos++) {
		if (strncmp(data + pos, needle, needle_len) == 0) {
			return true;
		}
	}
	return false;
}

static bool
slatejs_selector_attr_value_matches(const char *data, size_t data_len,
		const struct slatejs_selector_attr *attr)
{
	size_t pos = 0;

	switch (attr->op) {
	case SLATEJS_SELECTOR_ATTR_EXISTS:
		return true;
	case SLATEJS_SELECTOR_ATTR_EQUALS:
		return slatejs_selector_bytes_equal(data, data_len,
				attr->value, attr->value_len);
	case SLATEJS_SELECTOR_ATTR_PREFIX:
		return data_len >= attr->value_len &&
				strncmp(data, attr->value, attr->value_len) == 0;
	case SLATEJS_SELECTOR_ATTR_SUFFIX:
		return data_len >= attr->value_len &&
				strncmp(data + data_len - attr->value_len,
					attr->value, attr->value_len) == 0;
	case SLATEJS_SELECTOR_ATTR_CONTAINS:
		return slatejs_selector_bytes_contains(data, data_len,
				attr->value, attr->value_len);
	case SLATEJS_SELECTOR_ATTR_WORD:
		while (pos < data_len) {
			size_t start;
			size_t end;

			while (pos < data_len && isspace((unsigned char)data[pos])) {
				pos++;
			}
			start = pos;
			while (pos < data_len && !isspace((unsigned char)data[pos])) {
				pos++;
			}
			end = pos;
			if (slatejs_selector_bytes_equal(data + start, end - start,
					attr->value, attr->value_len)) {
				return true;
			}
		}
		return false;
	case SLATEJS_SELECTOR_ATTR_DASH:
		return slatejs_selector_bytes_equal(data, data_len,
				attr->value, attr->value_len) ||
				(data_len > attr->value_len &&
				 strncmp(data, attr->value, attr->value_len) == 0 &&
				 data[attr->value_len] == '-');
	}

	return false;
}

static bool
slatejs_selector_attr_matches(dom_element *element,
		const struct slatejs_selector_attr *attr)
{
	dom_string *attr_name = NULL;
	dom_string *attr_value = NULL;
	bool ret = false;

	if (attr->name == NULL || attr->name_len == 0) {
		return false;
	}

	if (dom_string_create((const uint8_t *)attr->name,
			attr->name_len, &attr_name) != DOM_NO_ERR) {
		return false;
	}

	if (dom_element_get_attribute(element, attr_name, &attr_value) !=
			DOM_NO_ERR || attr_value == NULL) {
		dom_string_unref(attr_name);
		return false;
	}

	ret = slatejs_selector_attr_value_matches(dom_string_data(attr_value),
			dom_string_length(attr_value), attr);

	dom_string_unref(attr_value);
	dom_string_unref(attr_name);
	return ret;
}

static bool
slatejs_selector_parse_attr(const char *selector, size_t selector_len,
		size_t *pos, struct slatejs_selector_attr *attr)
{
	size_t start = *pos + 1;
	size_t end = start;
	size_t eq;
	const char *name;
	size_t name_len;
	const char *value;
	size_t value_len;

	while (end < selector_len && selector[end] != ']') {
		end++;
	}
	if (end == selector_len) {
		return false;
	}

	name_len = end - start;
	name = slatejs_selector_trim(selector + start, &name_len);
	eq = 0;
	while (eq < name_len && name[eq] != '=') {
		eq++;
	}

	attr->name = name;
	attr->value = NULL;
	attr->value_len = 0;
	attr->op = SLATEJS_SELECTOR_ATTR_EXISTS;

	if (eq < name_len) {
		size_t name_part_len = eq;
		char op = '=';

		if (eq > 0 && (name[eq - 1] == '~' || name[eq - 1] == '|' ||
				name[eq - 1] == '^' || name[eq - 1] == '$' ||
				name[eq - 1] == '*')) {
			op = name[eq - 1];
			name_part_len--;
		}

		attr->name = slatejs_selector_trim(name, &name_part_len);
		attr->name_len = name_part_len;

		value_len = name_len - eq - 1;
		value = slatejs_selector_trim(name + eq + 1, &value_len);
		if (value_len >= 2 &&
				((value[0] == '"' && value[value_len - 1] == '"') ||
				 (value[0] == '\'' && value[value_len - 1] == '\''))) {
			value++;
			value_len -= 2;
		}

		attr->value = value;
		attr->value_len = value_len;
		switch (op) {
		case '~':
			attr->op = SLATEJS_SELECTOR_ATTR_WORD;
			break;
		case '|':
			attr->op = SLATEJS_SELECTOR_ATTR_DASH;
			break;
		case '^':
			attr->op = SLATEJS_SELECTOR_ATTR_PREFIX;
			break;
		case '$':
			attr->op = SLATEJS_SELECTOR_ATTR_SUFFIX;
			break;
		case '*':
			attr->op = SLATEJS_SELECTOR_ATTR_CONTAINS;
			break;
		default:
			attr->op = SLATEJS_SELECTOR_ATTR_EQUALS;
			break;
		}
	} else {
		attr->name = slatejs_selector_trim(name, &name_len);
		attr->name_len = name_len;
	}

	*pos = end + 1;
	return attr->name_len > 0;
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
	const char *classes[8];
	size_t class_lens[8];
	size_t class_count = 0;
	struct slatejs_selector_attr attrs[8];
	size_t attr_count = 0;
	size_t pos = 0;
	size_t start;
	bool ret;

	selector = slatejs_selector_trim(selector, &selector_len);
	if (selector_len == 0) {
		return false;
	}

	for (pos = 0; pos < selector_len; pos++) {
		if (selector[pos] == '[') {
			while (pos < selector_len && selector[pos] != ']') {
				pos++;
			}
			continue;
		}
		switch (selector[pos]) {
		case ' ':
		case '\t':
		case '\n':
		case '\r':
		case '>':
		case '+':
		case '~':
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
			selector[pos] != '.' && selector[pos] != '[') {
		pos++;
	}
	tag = selector + start;
	tag_len = pos - start;

	while (pos < selector_len) {
		char marker = selector[pos++];
		if (marker == '[') {
			pos--;
			if (attr_count >= 8 ||
					!slatejs_selector_parse_attr(selector,
						selector_len, &pos,
						&attrs[attr_count])) {
				return false;
			}
			attr_count++;
			continue;
		}

		start = pos;
		while (pos < selector_len && selector[pos] != '#' &&
				selector[pos] != '.' && selector[pos] != '[') {
			pos++;
		}
		if (marker == '#') {
			id = selector + start;
			id_len = pos - start;
		} else if (marker == '.') {
			if (class_count >= 8) {
				return false;
			}
			classes[class_count] = selector + start;
			class_lens[class_count] = pos - start;
			class_count++;
		} else {
			return false;
		}
	}

	if (tag_len == 0 && id == NULL && class_count == 0 &&
			attr_count == 0) {
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

	if (id != NULL) {
		struct slatejs_selector_attr id_attr = {
			.name = "id",
			.name_len = 2,
			.value = id,
			.value_len = id_len,
			.op = SLATEJS_SELECTOR_ATTR_EQUALS
		};
		if (!slatejs_selector_attr_matches((dom_element *)node, &id_attr)) {
			return false;
		}
	}

	for (pos = 0; pos < class_count; pos++) {
		if (!slatejs_selector_class_contains((dom_element *)node,
				classes[pos], class_lens[pos])) {
			return false;
		}
	}

	for (pos = 0; pos < attr_count; pos++) {
		if (!slatejs_selector_attr_matches((dom_element *)node, &attrs[pos])) {
			return false;
		}
	}

	return true;
}

static bool
slatejs_selector_prev_token(const char *start, const char **endp,
		const char **token, size_t *token_len, char *combinator)
{
	const char *end = *endp;
	const char *begin;
	unsigned int attr_depth = 0;
	char quote = '\0';

	while (end > start && isspace((unsigned char)end[-1])) {
		end--;
	}
	if (end > start && end[-1] == '>') {
		*combinator = '>';
		end--;
		while (end > start && isspace((unsigned char)end[-1])) {
			end--;
		}
	} else {
		*combinator = ' ';
	}
	if (end == start) {
		return false;
	}

	begin = end;
	while (begin > start) {
		char c = begin[-1];

		if (quote != '\0') {
			if (c == quote) {
				quote = '\0';
			}
			begin--;
			continue;
		}
		if (c == '"' || c == '\'') {
			quote = c;
			begin--;
			continue;
		}
		if (c == ']') {
			attr_depth++;
			begin--;
			continue;
		}
		if (c == '[' && attr_depth > 0) {
			attr_depth--;
			begin--;
			continue;
		}
		if (attr_depth == 0 &&
				(isspace((unsigned char)c) || c == '>')) {
			break;
		}
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
	char combinator;
	dom_node *cur = node;
	bool cur_owned = false;

	start = slatejs_selector_trim(selector, &selector_len);
	scan_end = start + selector_len;

	if (!slatejs_selector_prev_token(start, &scan_end,
			&token, &token_len, &combinator)) {
		return false;
	}

	if (!slatejs_selector_part_matches(cur, token, token_len)) {
		return false;
	}

	while (slatejs_selector_prev_token(start, &scan_end,
			&token, &token_len, &combinator)) {
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

		if (combinator == '>') {
			if (candidate != NULL &&
					slatejs_selector_part_matches(candidate,
						token, token_len)) {
				cur = candidate;
				cur_owned = true;
				found = true;
			} else if (candidate != NULL) {
				dom_node_unref(candidate);
			}
		} else {
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
	char *selector_copy = NULL;
	uint32_t length = 0;
	uint32_t out_idx = 0;
	uint32_t idx;

	if (dom_node_get_node_type(root, &root_type) != DOM_NO_ERR) {
		return 0;
	}

	selector_copy = malloc(selector_len + 1);
	if (selector_copy == NULL) {
		return 0;
	}
	memcpy(selector_copy, selector, selector_len);
	selector_copy[selector_len] = '\0';
	selector = selector_copy;

	if (dom_string_create((const uint8_t *)"*", 1, &star) != DOM_NO_ERR) {
		free(selector_copy);
		return 0;
	}

	if (root_type == DOM_DOCUMENT_NODE) {
		if (dom_document_get_elements_by_tag_name((dom_document *)root,
				star, &nodes) != DOM_NO_ERR) {
			dom_string_unref(star);
			free(selector_copy);
			return 0;
		}
	} else if (root_type == DOM_ELEMENT_NODE) {
		if (dom_element_get_elements_by_tag_name((dom_element *)root,
				star, &nodes) != DOM_NO_ERR) {
			dom_string_unref(star);
			free(selector_copy);
			return 0;
		}
	} else {
		dom_string_unref(star);
		free(selector_copy);
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
		free(selector_copy);
		return 0;
	}

	if (!first_only) {
		slatejs_push_array(ctx);
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
				free(selector_copy);
				return 1;
			}

			if (slatejs_bind_push_node(ctx, node)) {
				slatejs_put_prop_index(ctx, -2, out_idx++);
			}
		}
		dom_node_unref(node);
	}

	dom_nodelist_unref(nodes);
	free(selector_copy);
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
