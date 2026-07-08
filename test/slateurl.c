/*
 * Copyright 2015 Vincent Sanders <vince@netsurf-browser.org>
 * Copyright 2011 John Mark Bell <jmb@netsurf-browser.org>
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
 * Test slateurl operations.
 */

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <check.h>

#include <libwapcaplet/libwapcaplet.h>

#include "utils/corestrings.h"
#include "utils/slateurl.h"

#define NELEMS(x)  (sizeof(x) / sizeof((x)[0]))

struct test_pairs {
	const char* test;
	const char* res;
};

struct test_triplets {
	const char* test1;
	const char* test2;
	const char* res;
};

struct test_compare {
	const char* test1;
	const char* test2;
	slateurl_component parts;
	bool res;
};

/* Fixtures */

static void corestring_create(void)
{
	ck_assert(corestrings_init() == SLATEERROR_OK);
}

/**
 * iterator for any remaining strings in teardown fixture
 */
static void slate_lwc_iterator(lwc_string *str, void *pw)
{
	fprintf(stderr,
		"[%3u] %.*s",
		str->refcnt,
		(int)lwc_string_length(str),
		lwc_string_data(str));
}

static void corestring_teardown(void)
{
	corestrings_fini();

	lwc_iterate_strings(slate_lwc_iterator, NULL);
}

/* tests */

static const char *base_str = "http://a/b/c/d;p?q";

static const struct test_pairs create_tests[] = {
	{ "",			NULL },
	{ "http:",		NULL },
	{ "http:/",		NULL },
	{ "http://",		NULL },
	{ "http:a",		"http://a/" },
	{ "http:a/",		"http://a/" },
	{ "http:a/b",		"http://a/b" },
	{ "http:/a",		"http://a/" },
	{ "http:/a/b",		"http://a/b" },
	{ "http://a",		"http://a/" },
	{ "http://a/b",		"http://a/b" },
	{ "www.example.org",	"http://www.example.org/" },
	{ "www.example.org/x",	"http://www.example.org/x" },
	{ "about:",		"about:" },
	{ "about:blank",	"about:blank" },

	{ "http://www.ns-b.org:8080/",
	  "http://www.ns-b.org:8080/" },
	{ "http://user@www.ns-b.org:8080/hello",
	  "http://user@www.ns-b.org:8080/hello" },
	{ "http://user:pass@www.ns-b.org:8080/hello",
	  "http://user:pass@www.ns-b.org:8080/hello" },

	{ "http://www.ns-b.org:80/",
	  "http://www.ns-b.org/" },
	{ "http://user@www.ns-b.org:80/hello",
	  "http://user@www.ns-b.org/hello" },
	{ "http://user:pass@www.ns-b.org:80/hello",
	  "http://user:pass@www.ns-b.org/hello" },

	{ "http://www.ns-b.org:/",
	  "http://www.ns-b.org/" },
	{ "http://///////////www.ns-b.org:/",
	  "http://www.ns-b.org/" },
	{ "http://u@www.ns-b.org:/hello",
	  "http://u@www.ns-b.org/hello" },
	{ "http://u:p@www.ns-b.org:/hello",
	  "http://u:p@www.ns-b.org/hello" },

	{ "http:a/",		"http://a/" },
	{ "http:/a/",		"http://a/" },
	{ "http://u@a",		"http://u@a/" },
	{ "http://@a",		"http://a/" },

	{ "mailto:u@a",		"mailto:u@a" },
	{ "mailto:@a",		"mailto:a" },

	{ "file:///",		"file:///" },
	{ "file://",		"file:///" },
	{ "file:/",		"file:///" },
	{ "file:",		"file:///" },
	{ "file:////",		"file:////" },
	{ "file://///",		"file://///" },

	{ "file://localhost/",	"file:///" },
	{ "file://foobar/",	"file:///" },
	{ "file://foobar",	"file:///" },
	{ "file:///foobar",	"file:///foobar" },
	{ "file://tlsa@foo/",	"file:///" },

	/* test case insensitivity */
	{ "HTTP://a/b",		"http://a/b" },
	{ "HTTPS://a/b",	"https://a/b" },
	{ "ftp://a/b",		"ftp://a/b" },
	{ "FTP://a/b",		"ftp://a/b" },
	{ "MAILTO:foo@bar",	"mailto:foo@bar" },
	{ "FILE:///",		"file:///" },
	{ "http://HOST/",	"http://host/" },

	/* punycode */
	{ "http://a.कॉम/a", "http://a.xn--11b4c3d/a" },
	{ "https://smog.大众汽车/test", "https://smog.xn--3oq18vl8pn36a/test"},

	/* unnecessary escape */
	{ "http://%7a%7A/", "http://zz/" },

	/* bad escape */
	{ "http://%1g%G0/", NULL },

	{ "    http://www.ns-b.org/",		"http://www.ns-b.org/" },
	{ "http://www.ns-b.org/    ",		"http://www.ns-b.org/" },
	{ "http://www.ns-b.org    ",		"http://www.ns-b.org/" },
	{ "http://www.ns-b.org/?q   ",		"http://www.ns-b.org/?q" },
	{ "http://www.ns-b.org/#f    ",		"http://www.ns-b.org/#f" },

	/* Regression check from security report */
	{ "http://AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAfff",
	  "http://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaafff/"
	}
};

/**
 * url creation test
 */
START_TEST(slateurl_create_test)
{
	slateerror err;
	slateurl *res;
	const struct test_pairs *tst = &create_tests[_i];

	err = slateurl_create(tst->test, &res);
	if (tst->res == NULL) {
		/* result must be invalid */
		ck_assert(err != SLATEERROR_OK);

	} else {
		/* result must be valid */
		ck_assert(err == SLATEERROR_OK);

		ck_assert_str_eq(slateurl_access(res), tst->res);

		slateurl_unref(res);
	}
}
END_TEST

static const struct test_triplets access_tests[] = {
	{ "http://www.slate-browser.org/a/big/tree",
	  "http://www.slate-browser.org/a/big/tree",
	  "tree" },

	{ "HTTP://ci.slate-browser.org/jenkins/view/Unit Tests/job/coverage-netsurf/11/cobertura/utils/slateurl_c/",
	  "http://ci.slate-browser.org/jenkins/view/Unit%20Tests/job/coverage-netsurf/11/cobertura/utils/slateurl_c/",
	  "" },

	{ "FILE:///",
	  "file:///",
	  "/" },
};

/**
 * url access test
 */
START_TEST(slateurl_access_test)
{
	slateerror err;
	slateurl *res_url;
	const struct test_triplets *tst = &access_tests[_i];

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test1, &res_url);
	ck_assert(err == SLATEERROR_OK);

	/* The url accessed string must match the input */
	ck_assert_str_eq(slateurl_access(res_url), tst->test2);

	slateurl_unref(res_url);
}
END_TEST

/**
 * url access leaf test
 */
START_TEST(slateurl_access_leaf_test)
{
	slateerror err;
	slateurl *res_url;
	const struct test_triplets *tst = &access_tests[_i];

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test1, &res_url);
	ck_assert(err == SLATEERROR_OK);

	ck_assert_str_eq(slateurl_access_leaf(res_url), tst->res);

	slateurl_unref(res_url);

}
END_TEST

/**
 * url length test
 *
 * uses access dataset and test unit
 */
START_TEST(slateurl_length_test)
{
	slateerror err;
	slateurl *res_url;
	const struct test_triplets *tst = &access_tests[_i];

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test1, &res_url);
	ck_assert(err == SLATEERROR_OK);

	ck_assert_int_eq(slateurl_length(res_url), strlen(tst->test2));

	slateurl_unref(res_url);

}
END_TEST


static const struct test_pairs nice_tests[] = {
	{ "about:",			NULL },
	{ "www.foo.org",		"www_foo_org" },
	{ "www.foo.org/index.html",	"www_foo_org" },
	{ "www.foo.org/default.en",	"www_foo_org" },
	{ "www.foo.org/about",		"about" },
	{ "www.foo.org/about.jpg",	"about.jpg" },
	{ "www.foo.org/moose/index.en",	"moose" },
	{ "www.foo.org/a//index.en",	"www_foo_org" },
	{ "www.foo.org/a//index.en",	"www_foo_org" },
	{ "http://www.f.org//index.en",	"www_f_org" },
};

/**
 * url nice filename without stripping
 */
START_TEST(slateurl_nice_nostrip_test)
{
	slateerror err;
	slateurl *res_url;
	char *res_str;
	const struct test_pairs *tst = &nice_tests[_i];

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test, &res_url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_nice(res_url, &res_str, false);
	if (tst->res == NULL) {
		/* result must be invalid (bad input) */
		ck_assert(err != SLATEERROR_OK);
	} else {
		/* result must be valid */
		ck_assert(err == SLATEERROR_OK);

		ck_assert_str_eq(res_str, tst->res);

		free(res_str);
	}
	slateurl_unref(res_url);

}
END_TEST


static const struct test_pairs nice_strip_tests[] = {
	{ "about:",			NULL },
	{ "www.foo.org",		"www_foo_org" },
	{ "www.foo.org/index.html",	"www_foo_org" },
	{ "www.foo.org/default.en",	"www_foo_org" },
	{ "www.foo.org/about",		"about" },
	{ "www.foo.org/about.jpg",	"about" },
	{ "www.foo.org/moose/index.en",	"moose" },
	{ "www.foo.org/a//index.en",	"www_foo_org" },
	{ "www.foo.org/a//index.en",	"www_foo_org" },
	{ "http://www.f.org//index.en",	"www_f_org" },
};

/**
 * url nice filename with stripping
 */
START_TEST(slateurl_nice_strip_test)
{
	slateerror err;
	slateurl *res_url;
	char *res_str;
	const struct test_pairs *tst = &nice_strip_tests[_i];

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test, &res_url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_nice(res_url, &res_str, true);
	if (tst->res == NULL) {
		/* result must be invalid (bad input) */
		ck_assert(err != SLATEERROR_OK);
	} else {
		/* result must be valid */
		ck_assert(err == SLATEERROR_OK);

		ck_assert_str_eq(res_str, tst->res);

		free(res_str);
	}
	slateurl_unref(res_url);

}
END_TEST


/**
 * simple joins that all use http://a/b/c/d;p?q as a base
 */
static const struct test_pairs join_tests[] = {
	/* Normal Examples rfc3986 5.4.1 */
	{ "g:h",		"g:h" },
	{ "g",			"http://a/b/c/g" },
	{ "./g",		"http://a/b/c/g" },
	{ "g/",			"http://a/b/c/g/" },
	{ "/g",			"http://a/g" },
	{ "//g",		"http://g" /* [1] */ "/" },
	{ "?y",			"http://a/b/c/d;p?y" },
	{ "g?y",		"http://a/b/c/g?y" },
	{ "#s",			"http://a/b/c/d;p?q#s" },
	{ "g#s",		"http://a/b/c/g#s" },
	{ "g?y#s",		"http://a/b/c/g?y#s" },
	{ ";x",			"http://a/b/c/;x" },
	{ "g;x",		"http://a/b/c/g;x" },
	{ "g;x?y#s",		"http://a/b/c/g;x?y#s" },
	{ "",			"http://a/b/c/d;p?q" },
	{ ".",			"http://a/b/c/" },
	{ "./",			"http://a/b/c/" },
	{ "..",			"http://a/b/" },
	{ "../",		"http://a/b/" },
	{ "../g",		"http://a/b/g" },
	{ "../..",		"http://a/" },
	{ "../../",		"http://a/" },
	{ "../../g",		"http://a/g" },

	/* Abnormal Examples rfc3986 5.4.2 */
	{ "../../../g",		"http://a/g" },
	{ "../../../../g",	"http://a/g" },

	{ "/./g",		"http://a/g" },
	{ "/../g",		"http://a/g" },
	{ "g.",			"http://a/b/c/g." },
	{ ".g",			"http://a/b/c/.g" },
	{ "g..",		"http://a/b/c/g.." },
	{ "..g",		"http://a/b/c/..g" },

	{ "./../g",		"http://a/b/g" },
	{ "./g/.",		"http://a/b/c/g/" },
	{ "g/./h",		"http://a/b/c/g/h" },
	{ "g/../h",		"http://a/b/c/h" },
	{ "g;x=1/./y",		"http://a/b/c/g;x=1/y" },
	{ "g;x=1/../y",		"http://a/b/c/y" },

	{ "g?y/./x",		"http://a/b/c/g?y/./x" },
	{ "g?y/../x",		"http://a/b/c/g?y/../x" },
	{ "g#s/./x",		"http://a/b/c/g#s/./x" },
	{ "g#s/../x",		"http://a/b/c/g#s/../x" },

	{ "http:g",		"http:g" /* [2] */ },

	/* Extra tests */
	{ " g",			"http://a/b/c/g" },
	{ "g ",			"http://a/b/c/g" },
	{ " g ",		"http://a/b/c/g" },
	{ "http:/b/c",		"http://b/c" },
	{ "http://",		"http:" },
	{ "http:/",		"http:" },
	{ "http:",		"http:" },
	{ " ",			"http://a/b/c/d;p?q" },
	{ "  ",			"http://a/b/c/d;p?q" },
	{ "/",			"http://a/" },
	{ "  /  ",		"http://a/" },
	{ "  ?  ",		"http://a/b/c/d;p" },
	{ "  h  ",		"http://a/b/c/h" },
	{ "//foo?",		"http://foo/" },
	{ "//foo#bar",		"http://foo/#bar" },
	{ "//foo/",		"http://foo/" },
	{ "http://<!--#echo var=", "http://<!--/#echo%20var="},
	/* [1] Extra slash beyond rfc3986 5.4.1 example, since we're
	 *     testing normalisation in addition to joining */
	/* [2] Using the strict parsers option */

};

/**
 * url joining
 */
START_TEST(slateurl_join_test)
{
	slateerror err;
	slateurl *base_url;
	slateurl *joined;
	char *string;
	size_t len;
	const struct test_pairs *tst = &join_tests[_i];

	/* not testing create, this should always succeed */
	err = slateurl_create(base_str, &base_url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_join(base_url, tst->test, &joined);
	if (tst->res == NULL) {
		/* result must be invalid (bad input) */
		ck_assert(err != SLATEERROR_OK);
	} else {
		/* result must be valid */
		ck_assert(err == SLATEERROR_OK);

		err = slateurl_get(joined, SLATEURL_WITH_FRAGMENT, &string, &len);
		ck_assert(err == SLATEERROR_OK);

		ck_assert_str_eq(string, tst->res);

		free(string);
		slateurl_unref(joined);
	}
	slateurl_unref(base_url);

}
END_TEST


/**
 * more complex joins that specify a base to join to
 */
static const struct test_triplets join_complex_tests[] = {
	/* problematic real world urls for regression */
	{ "http://www.bridgetmckenna.com/blog/self-editing-for-everyone-part-1-the-most-hated-writing-advice-ever",
	  "http://The%20Old%20Organ%20Trail%20http://www.amazon.com/gp/product/B007B57MCQ/ref=as_li_tf_tl?ie=UTF8&camp=1789&creative=9325&creativeASIN=B007B57MCQ&linkCode=as2&tag=brimck0f-20",
	  "http://the old organ trail http:" },
};

/**
 * complex url joining
 */
START_TEST(slateurl_join_complex_test)
{
	slateerror err;
	slateurl *base_url;
	slateurl *joined;
	char *string;
	size_t len;
	const struct test_triplets *tst = &join_complex_tests[_i];

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test1, &base_url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_join(base_url, tst->test2, &joined);
	if (tst->res == NULL) {
		/* result must be invalid (bad input) */
		ck_assert(err != SLATEERROR_OK);
	} else {
		/* result must be valid */
		ck_assert(err == SLATEERROR_OK);

		err = slateurl_get(joined, SLATEURL_WITH_FRAGMENT, &string, &len);
		ck_assert(err == SLATEERROR_OK);

		ck_assert_str_eq(string, tst->res);

		free(string);
		slateurl_unref(joined);
	}
	slateurl_unref(base_url);

}
END_TEST


/**
 * query replacement tests
 */
static const struct test_triplets replace_query_tests[] = {
	{ "http://netsurf-browser.org/?magical=true",
	  "magical=true&result=win",
	  "http://netsurf-browser.org/?magical=true&result=win"},

	{ "http://netsurf-browser.org/?magical=true#fragment",
	  "magical=true&result=win",
	  "http://netsurf-browser.org/?magical=true&result=win#fragment"},

	{ "http://netsurf-browser.org/#fragment",
	  "magical=true&result=win",
	  "http://netsurf-browser.org/?magical=true&result=win#fragment"},

	{ "http://netsurf-browser.org/path",
	  "magical=true",
	  "http://netsurf-browser.org/path?magical=true"},

	{ "http://netsurf-browser.org/path?magical=true",
	  "",
	  "http://netsurf-browser.org/path"},

};

/**
 * replace query
 */
START_TEST(slateurl_replace_query_test)
{
	slateerror err;
	slateurl *res_url;
	slateurl *joined;
	const struct test_triplets *tst = &replace_query_tests[_i];

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test1, &res_url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_replace_query(res_url, tst->test2, &joined);
	if (tst->res == NULL) {
		/* result must be invalid (bad input) */
		ck_assert(err != SLATEERROR_OK);
	} else {
		/* result must be valid */
		ck_assert(err == SLATEERROR_OK);

		ck_assert_str_eq(slateurl_access(joined), tst->res);

		slateurl_unref(joined);
	}
	slateurl_unref(res_url);

}
END_TEST


/**
 * url comparison tests
 */
static const struct test_compare compare_tests[] = {
	{ "http://a/b/c/d;p?q",
	  "http://a/b/c/d;p?q",
	  SLATEURL_WITH_FRAGMENT,
	  true },

	{ "http://a.b.c/d?a",
	  "http://a.b.c/e?a",
	  SLATEURL_WITH_FRAGMENT,
	  false },

	{ "http://a.b.c/",
	  "http://g.h.i/",
	  SLATEURL_WITH_FRAGMENT,
	  false },

	{ "http://a.b.c/d?a",
	  "http://a.b.c/d?b",
	  SLATEURL_WITH_FRAGMENT,
	  false },

	{ "http://a.b.c/d?a",
	  "https://a.b.c/d?a",
	  SLATEURL_WITH_FRAGMENT,
	  false },
};

/**
 * compare
 */
START_TEST(slateurl_compare_test)
{
	slateerror err;
	slateurl *url1;
	slateurl *url2;
	const struct test_compare *tst = &compare_tests[_i];
	bool status;

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test1, &url1);
	ck_assert(err == SLATEERROR_OK);

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test2, &url2);
	ck_assert(err == SLATEERROR_OK);

	status = slateurl_compare(url1, url2, tst->parts);
	ck_assert(status == tst->res);

	slateurl_unref(url1);
	slateurl_unref(url2);

}
END_TEST


/* component test case */

/**
 * url component tests
 *
 * each test1 parameter is converted to a url and
 * slateurl[get|has]_component called on it with the given part. The
 * result is checked against test1 and res as approprite.
 */
static const struct test_compare component_tests[] = {
	{ "http://u:p@a:66/b/c/d;p?q#f", "http", SLATEURL_SCHEME, true },
	{ "http://u:p@a:66/b/c/d;p?q#f", "u", SLATEURL_USERNAME, true },
	{ "http://u:p@a:66/b/c/d;p?q#f", "p", SLATEURL_PASSWORD, true },
	{ "http://u:p@a:66/b/c/d;p?q#f", "a", SLATEURL_HOST, true },
	{ "http://u:p@a:66/b/c/d;p?q#f", "66", SLATEURL_PORT, true },
	{ "http://u:p@a:66/b/c/d;p?q#f", "/b/c/d;p", SLATEURL_PATH, true },
	{ "http://u:p@a:66/b/c/d;p?q#f", "q", SLATEURL_QUERY, true },
	{ "http://u:p@a:66/b/c/d;p?q#f", "f", SLATEURL_FRAGMENT, true },

	{ "file:", "file", SLATEURL_SCHEME, true },
	{ "file:", NULL, SLATEURL_USERNAME, false },
	{ "file:", NULL, SLATEURL_PASSWORD, false },
	{ "file:", NULL, SLATEURL_HOST, false },
	{ "file:", NULL, SLATEURL_PORT, false },
	{ "file:", "/", SLATEURL_PATH, true },
	{ "file:", NULL, SLATEURL_QUERY, false },
	{ "file:", NULL, SLATEURL_FRAGMENT, false },

	{ "http://u:p@a:66/b/c/d;p?q=v#f", "q=v", SLATEURL_QUERY, true },
	{ "http://u:p@a:66/b/c/d;p?q=v", "q=v", SLATEURL_QUERY, true },
	{ "http://u:p@a:66/b/c/d;p?q=v&q1=v1#f", "q=v&q1=v1", SLATEURL_QUERY, true },
	{ "http://u:p@a:66/b/c/d;p?q=v&q1=v1", "q=v&q1=v1", SLATEURL_QUERY, true },

};


/**
 * get component
 */
START_TEST(slateurl_get_component_test)
{
	slateerror err;
	slateurl *url1;
	const struct test_compare *tst = &component_tests[_i];
	lwc_string *cmpnt;

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test1, &url1);
	ck_assert(err == SLATEERROR_OK);

	cmpnt = slateurl_get_component(url1, tst->parts);
	if (cmpnt == NULL) {
		ck_assert(tst->test2 == NULL);
	} else {
		ck_assert_str_eq(lwc_string_data(cmpnt), tst->test2);
		lwc_string_unref(cmpnt);
	}

	slateurl_unref(url1);
}
END_TEST


/**
 * has component
 */
START_TEST(slateurl_has_component_test)
{
	slateerror err;
	slateurl *url1;
	const struct test_compare *tst = &component_tests[_i];
	bool status;

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test1, &url1);
	ck_assert(err == SLATEERROR_OK);

	status = slateurl_has_component(url1, tst->parts);
	ck_assert(status == tst->res);

	slateurl_unref(url1);
}
END_TEST


/**
 * test case for componnet get and has API
 */
static TCase *slateurl_component_case_create(void)
{
	TCase *tc;
	tc = tcase_create("Component");

	tcase_add_unchecked_fixture(tc,
				    corestring_create,
				    corestring_teardown);

	tcase_add_loop_test(tc,
			    slateurl_get_component_test,
			    0, NELEMS(component_tests));
	tcase_add_loop_test(tc,
			    slateurl_has_component_test,
			    0, NELEMS(component_tests));

	return tc;
}


static const struct test_pairs fragment_tests[] = {
	{ "http://www.f.org/a/b/c#def", "http://www.f.org/a/b/c" },
};

/**
 * defragment url
 */
START_TEST(slateurl_defragment_test)
{
	slateerror err;
	slateurl *url;
	slateurl *res_url;
	const struct test_pairs *tst = &fragment_tests[_i];

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test, &url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_defragment(url, &res_url);
	if (tst->res == NULL) {
		/* result must be invalid (bad input) */
		ck_assert(err != SLATEERROR_OK);
	} else {
		/* result must be valid */
		ck_assert(err == SLATEERROR_OK);

		ck_assert_str_eq(slateurl_access(res_url), tst->res);

		slateurl_unref(res_url);
	}
	slateurl_unref(url);

}
END_TEST

/**
 * refragment url
 */
START_TEST(slateurl_refragment_test)
{
	slateerror err;
	slateurl *url;
	slateurl *res_url;
	const struct test_pairs *tst = &fragment_tests[_i];
	lwc_string *frag;

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test, &url);
	ck_assert(err == SLATEERROR_OK);

	/* grab the fragment - not testing should succeed */
	frag = slateurl_get_component(url, SLATEURL_FRAGMENT);
	ck_assert(frag != NULL);
	slateurl_unref(url);

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->res, &url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_refragment(url, frag, &res_url);
	if (tst->res == NULL) {
		/* result must be invalid (bad input) */
		ck_assert(err != SLATEERROR_OK);
	} else {
		/* result must be valid */
		ck_assert(err == SLATEERROR_OK);

		ck_assert_str_eq(slateurl_access(res_url), tst->test);

		slateurl_unref(res_url);
	}

	lwc_string_unref(frag);

	slateurl_unref(url);
}
END_TEST



/**
 * url reference (copy) and unreference(free)
 */
START_TEST(slateurl_ref_test)
{
	slateerror err;
	slateurl *res1;
	slateurl *res2;

	err = slateurl_create(base_str, &res1);

	/* result must be valid */
	ck_assert(err == SLATEERROR_OK);

	res2 = slateurl_ref(res1);

	ck_assert_str_eq(slateurl_access(res1), slateurl_access(res2));

	slateurl_unref(res2);

	slateurl_unref(res1);
}
END_TEST


/**
 * check creation asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_create_test)
{
	slateerror err;
	slateurl *res1;
	err = slateurl_create(NULL, &res1);

	ck_assert(err != SLATEERROR_OK);
}
END_TEST

/**
 * check ref asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_ref_test)
{
	slateurl_ref(NULL);
}
END_TEST

/**
 * check unref asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_unref_test)
{
	slateurl_unref(NULL);
}
END_TEST

/**
 * check compare asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_compare1_test)
{
	slateerror err;
	slateurl *res;
	bool same;

	err = slateurl_create(base_str, &res);
	ck_assert(err == SLATEERROR_OK);

	same = slateurl_compare(NULL, res, SLATEURL_PATH);

	ck_assert(same == false);

	slateurl_unref(res);
}
END_TEST

/**
 * check compare asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_compare2_test)
{
	slateerror err;
	slateurl *res;
	bool same;

	err = slateurl_create(base_str, &res);
	ck_assert(err == SLATEERROR_OK);

	same = slateurl_compare(res, NULL, SLATEURL_PATH);

	ck_assert(same == false);
}
END_TEST

/**
 * check get asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_get_test)
{
	slateerror err;
	char *url_s = NULL;
	size_t url_l = 0;

	err = slateurl_get(NULL, SLATEURL_PATH, &url_s, &url_l);
	ck_assert(err != SLATEERROR_OK);
	ck_assert(url_s == NULL);
	ck_assert(url_l == 0);
}
END_TEST

/**
 * check get component asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_get_component1_test)
{
	lwc_string *lwcs;

	lwcs = slateurl_get_component(NULL, SLATEURL_PATH);
	ck_assert(lwcs == NULL);
}
END_TEST

/**
 * check get component asserts on bad component parameter
 */
START_TEST(slateurl_api_assert_get_component2_test)
{
	slateerror err;
	slateurl *res;
	lwc_string *lwcs;

	err = slateurl_create(base_str, &res);
	ck_assert(err == SLATEERROR_OK);

	lwcs = slateurl_get_component(res, -1);
	ck_assert(lwcs == NULL);

	slateurl_unref(res);
}
END_TEST

/**
 * check has component asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_has_component1_test)
{
	bool has;

	has = slateurl_has_component(NULL, SLATEURL_PATH);
	ck_assert(has == false);
}
END_TEST

/**
 * check has component asserts on bad component parameter
 */
START_TEST(slateurl_api_assert_has_component2_test)
{
	slateerror err;
	slateurl *res;
	bool has;

	err = slateurl_create(base_str, &res);
	ck_assert(err == SLATEERROR_OK);

	has = slateurl_has_component(res, -1);
	ck_assert(has == false);

	slateurl_unref(res);
}
END_TEST


/**
 * check access asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_access_test)
{
	const char *res_s = NULL;

	res_s = slateurl_access(NULL);

	ck_assert(res_s == NULL);
}
END_TEST

/**
 * check access asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_access_leaf_test)
{
	const char *res_s = NULL;

	res_s = slateurl_access_leaf(NULL);

	ck_assert(res_s == NULL);
}
END_TEST

/**
 * check length asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_length_test)
{
	size_t res = 0;

	res = slateurl_length(NULL);

	ck_assert(res == 0);
}
END_TEST

/**
 * check hash asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_hash_test)
{
	uint32_t res = 0;

	res = slateurl_hash(NULL);

	ck_assert(res == 0);
}
END_TEST

/**
 * check join asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_join1_test)
{
	const char *rel = "moo";
	slateurl *res;
	slateerror err;

	err = slateurl_join(NULL, rel, &res);
	ck_assert(err != SLATEERROR_OK);
}
END_TEST

/**
 * check join asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_join2_test)
{
	slateurl *url;
	slateurl *res;
	slateerror err;

	err = slateurl_create(base_str, &url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_join(url, NULL, &res);
	ck_assert(err != SLATEERROR_OK);

	slateurl_unref(url);
}
END_TEST

/**
 * check defragment asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_defragment_test)
{
	slateurl *res;
	slateerror err;

	err = slateurl_defragment(NULL, &res);
	ck_assert(err != SLATEERROR_OK);
}
END_TEST


/**
 * check refragment join asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_refragment1_test)
{
	slateurl *res;
	slateerror err;

	err = slateurl_refragment(NULL, corestring_lwc_http, &res);
	ck_assert(err != SLATEERROR_OK);
}
END_TEST

/**
 * check refragment asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_refragment2_test)
{
	slateurl *url;
	slateurl *res;
	slateerror err;

	err = slateurl_create(base_str, &url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_refragment(url, NULL, &res);
	ck_assert(err != SLATEERROR_OK);

	slateurl_unref(url);
}
END_TEST

/**
 * check query replacement asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_replace_query1_test)
{
	const char *rel = "moo";
	slateurl *res;
	slateerror err;

	err = slateurl_replace_query(NULL, rel, &res);
	ck_assert(err != SLATEERROR_OK);
}
END_TEST

/**
 * check query replacement asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_replace_query2_test)
{
	slateurl *url;
	slateurl *res;
	slateerror err;

	err = slateurl_create(base_str, &url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_replace_query(url, NULL, &res);
	ck_assert(err != SLATEERROR_OK);

	slateurl_unref(url);
}
END_TEST

/**
 * check query replacement asserts on bad parameter
 */
START_TEST(slateurl_api_assert_replace_query3_test)
{
	slateurl *url;
	slateurl *res;
	slateerror err;

	err = slateurl_create(base_str, &url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_replace_query(url, NULL, &res);
	ck_assert(err != SLATEERROR_OK);

	slateurl_unref(url);
}
END_TEST

/**
 * check nice asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_nice_test)
{
	char *res_s = NULL;
	slateerror err;

	err = slateurl_nice(NULL, &res_s, false);
	ck_assert(err != SLATEERROR_OK);

	ck_assert(res_s == NULL);
}
END_TEST


/**
 * check parent asserts on NULL parameter
 */
START_TEST(slateurl_api_assert_parent_test)
{
	slateurl *res;
	slateerror err;

	err = slateurl_parent(NULL, &res);
	ck_assert(err != SLATEERROR_OK);
}
END_TEST




/* parent test case */

static const struct test_pairs parent_tests[] = {
	{ "http://www.f.org/a/b/c", "http://www.f.org/a/b/" },
	{ "https://www.moo.org/", "https://www.moo.org/" },
	{ "https://www.moo.org/asinglepathelementthatsquitelong/", "https://www.moo.org/" },
	{ "https://user:pw@www.moo.org/a/b#x?a=b", "https://user:pw@www.moo.org/a/" },
};

/**
 * generate parent url
 */
START_TEST(slateurl_parent_test)
{
	slateerror err;
	slateurl *url;
	slateurl *res_url;
	const struct test_pairs *tst = &parent_tests[_i];

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test, &url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_parent(url, &res_url);
	if (tst->res == NULL) {
		/* result must be invalid (bad input) */
		ck_assert(err != SLATEERROR_OK);
	} else {
		/* result must be valid */
		ck_assert(err == SLATEERROR_OK);

		ck_assert_str_eq(slateurl_access(res_url), tst->res);

		slateurl_unref(res_url);
	}
	slateurl_unref(url);

}
END_TEST


/**
 * test case for parent API
 */
static TCase *slateurl_parent_case_create(void)
{
	TCase *tc;
	tc = tcase_create("Parent");

	tcase_add_unchecked_fixture(tc,
				    corestring_create,
				    corestring_teardown);

	tcase_add_loop_test(tc,
			    slateurl_parent_test,
			    0, NELEMS(parent_tests));

	return tc;
}


/* utf8 test case */

/**
 * utf8 tests
 */
static const struct test_pairs utf8_tests[] = {
	{ "http://a.xn--11b4c3d/a", "http://a.कॉम/a"  },
	{ "https://smog.xn--3oq18vl8pn36a/test", "https://smog.大众汽车/test"},


	/* Regression check from security report */
	{ "http://AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAfff",
	  "http://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaafff/"
	}
};


/**
 * get utf8 test
 */
START_TEST(slateurl_get_utf8_test)
{
	slateerror err;
	slateurl *url;
	const struct test_pairs *tst = &utf8_tests[_i];
	char *utf8out;
	size_t utf8out_len;
	size_t tstres_len;

	/* not testing create, this should always succeed */
	err = slateurl_create(tst->test, &url);
	ck_assert(err == SLATEERROR_OK);

	err = slateurl_get_utf8(url, &utf8out, &utf8out_len);
	ck_assert(err == SLATEERROR_OK);

	/* ensure length is correct */
	tstres_len = strlen(tst->res);
	ck_assert_uint_eq(tstres_len, utf8out_len);

	/* ensure string matches */
	ck_assert_str_eq(utf8out, tst->res);

	free(utf8out);

	slateurl_unref(url);
}
END_TEST


/**
 * test case for utf8 output
 */
static TCase *slateurl_utf8_case_create(void)
{
	TCase *tc;
	tc = tcase_create("UTF-8 output");

	tcase_add_unchecked_fixture(tc,
				    corestring_create,
				    corestring_teardown);

	tcase_add_loop_test(tc,
			    slateurl_get_utf8_test,
			    0, NELEMS(utf8_tests));

	return tc;
}


/* test suite */

/**
 * slateurl suite generation
 */
static Suite *slateurl_suite(void)
{
	Suite *s;
	TCase *tc_api_assert;
	TCase *tc_create;
	TCase *tc_access;
	TCase *tc_nice_nostrip;
	TCase *tc_nice_strip;
	TCase *tc_replace_query;
	TCase *tc_join;
	TCase *tc_compare;
	TCase *tc_fragment;

	s = suite_create("slateurl");

	/* Basic API operation assert checks */
	tc_api_assert = tcase_create("API asserts");

	tcase_add_unchecked_fixture(tc_api_assert,
				    corestring_create,
				    corestring_teardown);

	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_create_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_ref_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_unref_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_compare1_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_compare2_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_get_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_get_component1_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_get_component2_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_has_component1_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_has_component2_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_access_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_access_leaf_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_length_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_hash_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_join1_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_join2_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_defragment_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_refragment1_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_refragment2_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_replace_query1_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_replace_query2_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_replace_query3_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_nice_test, 6);
	tcase_add_test_raise_signal(tc_api_assert,
				    slateurl_api_assert_parent_test, 6);

	suite_add_tcase(s, tc_api_assert);

	/* url creation */
	tc_create = tcase_create("Create");

	tcase_add_unchecked_fixture(tc_create,
				    corestring_create,
				    corestring_teardown);

	tcase_add_loop_test(tc_create,
			    slateurl_create_test,
			    0, NELEMS(create_tests));
	tcase_add_test(tc_create, slateurl_ref_test);
	suite_add_tcase(s, tc_create);

	/* url access and length */
	tc_access = tcase_create("Access");

	tcase_add_unchecked_fixture(tc_access,
				    corestring_create,
				    corestring_teardown);
	tcase_add_loop_test(tc_access,
			    slateurl_access_test,
			    0, NELEMS(access_tests));
	tcase_add_loop_test(tc_access,
			    slateurl_access_leaf_test,
			    0, NELEMS(access_tests));
	tcase_add_loop_test(tc_access,
			    slateurl_length_test,
			    0, NELEMS(access_tests));
	suite_add_tcase(s, tc_access);

	/* nice filename without strip */
	tc_nice_nostrip = tcase_create("Nice (nostrip)");

	tcase_add_unchecked_fixture(tc_nice_nostrip,
				    corestring_create,
				    corestring_teardown);

	tcase_add_loop_test(tc_nice_nostrip,
			    slateurl_nice_nostrip_test,
			    0, NELEMS(nice_tests));
	suite_add_tcase(s, tc_nice_nostrip);


	/* nice filename with strip */
	tc_nice_strip = tcase_create("Nice (strip)");

	tcase_add_unchecked_fixture(tc_nice_strip,
				    corestring_create,
				    corestring_teardown);

	tcase_add_loop_test(tc_nice_strip,
			    slateurl_nice_strip_test,
			    0, NELEMS(nice_strip_tests));
	suite_add_tcase(s, tc_nice_strip);


	/* replace query */
	tc_replace_query = tcase_create("Replace Query");

	tcase_add_unchecked_fixture(tc_replace_query,
				    corestring_create,
				    corestring_teardown);

	tcase_add_loop_test(tc_replace_query,
			    slateurl_replace_query_test,
			    0, NELEMS(replace_query_tests));
	suite_add_tcase(s, tc_replace_query);

	/* url join */
	tc_join = tcase_create("Join");

	tcase_add_unchecked_fixture(tc_join,
				    corestring_create,
				    corestring_teardown);

	tcase_add_loop_test(tc_join,
			    slateurl_join_test,
			    0, NELEMS(join_tests));
	tcase_add_loop_test(tc_join,
			    slateurl_join_complex_test,
			    0, NELEMS(join_complex_tests));

	suite_add_tcase(s, tc_join);


	/* url compare */
	tc_compare = tcase_create("Compare");

	tcase_add_unchecked_fixture(tc_compare,
				    corestring_create,
				    corestring_teardown);

	tcase_add_loop_test(tc_compare,
			    slateurl_compare_test,
			    0, NELEMS(compare_tests));

	suite_add_tcase(s, tc_compare);

	/* fragment */
	tc_fragment = tcase_create("Fragment");

	tcase_add_unchecked_fixture(tc_fragment,
				    corestring_create,
				    corestring_teardown);

	tcase_add_loop_test(tc_fragment,
			    slateurl_defragment_test,
			    0, NELEMS(fragment_tests));
	tcase_add_loop_test(tc_fragment,
			    slateurl_refragment_test,
			    0, NELEMS(fragment_tests));

	suite_add_tcase(s, tc_fragment);


	/* component */
	suite_add_tcase(s, slateurl_component_case_create());


	/* parent */
	suite_add_tcase(s, slateurl_parent_case_create());

	/* UTF-8 output */
	suite_add_tcase(s, slateurl_utf8_case_create());


	return s;
}


int main(int argc, char **argv)
{
	int number_failed;
	Suite *s;
	SRunner *sr;

	s = slateurl_suite();

	sr = srunner_create(s);
	srunner_run_all(sr, CK_ENV);

	number_failed = srunner_ntests_failed(sr);
	srunner_free(sr);

	return (number_failed == 0) ? EXIT_SUCCESS : EXIT_FAILURE;
}
