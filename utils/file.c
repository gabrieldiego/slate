/*
 * Copyright 2014 vincent Sanders <vince@netsurf-browser.org>
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
 * Table operations for files with posix compliant path separator.
 */

#include <stdarg.h>
#include <string.h>
#include <stdio.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>

#include "desktop/gui_internal.h"

#include "utils/utils.h"
#include "utils/corestrings.h"
#include "utils/url.h"
#include "utils/slateurl.h"
#include "utils/string.h"
#include "utils/file.h"
#include "utils/dirent.h"

#ifdef slateamiga
#include "frontends/amiga/os3support.h"
#endif

/**
 * Generate a posix path from one or more component elemnts.
 *
 * If a string is allocated it must be freed by the caller.
 *
 * @param[in,out] str pointer to string pointer if this is NULL enough
 *                    storage will be allocated for the complete path.
 * @param[in,out] size The size of the space available if \a str not
 *                     NULL on input and if not NULL set to the total
 *                     output length on output.
 * @param[in] nelm The number of elements.
 * @param[in] ap The elements of the path as string pointers.
 * @return SLATEERROR_OK and the complete path is written to str
 *         or error code on faliure.
 */
static slateerror posix_vmkpath(char **str, size_t *size, size_t nelm, va_list ap)
{
	return vsnstrjoin(str, size, '/', nelm, ap);
}

/**
 * Get the basename of a file using posix path handling.
 *
 * This gets the last element of a path and returns it.
 *
 * @param[in] path The path to extract the name from.
 * @param[in,out] str Pointer to string pointer if this is NULL enough
 *                    storage will be allocated for the path element.
 * @param[in,out] size The size of the space available if \a
 *                     str not NULL on input and set to the total
 *                     output length on output.
 * @return SLATEERROR_OK and the complete path is written to str
 *         or error code on faliure.
 */
static slateerror posix_basename(const char *path, char **str, size_t *size)
{
	const char *leafname;
	char *fname;

	if (path == NULL) {
		return SLATEERROR_BAD_PARAMETER;
	}

	leafname = strrchr(path, '/');
	if (!leafname) {
		leafname = path;
	} else {
		leafname += 1;
	}

	fname = strdup(leafname);
	if (fname == NULL) {
		return SLATEERROR_NOMEM;
	}

	*str = fname;
	if (size != NULL) {
		*size = strlen(fname);
	}
	return SLATEERROR_OK;
}

/**
 * Create a path from a slateurl using posix file handling.
 *
 * @param[in] url The url to encode.
 * @param[out] path_out A string containing the result path which should
 *                      be freed by the caller.
 * @return SLATEERROR_OK and the path is written to \a path or error code
 *         on faliure.
 */
static slateerror posix_slateurl_to_path(struct slateurl *url, char **path_out)
{
	lwc_string *urlpath;
	char *path;
	bool match;
	lwc_string *scheme;
	slateerror res;

	if ((url == NULL) || (path_out == NULL)) {
		return SLATEERROR_BAD_PARAMETER;
	}

	scheme = slateurl_get_component(url, SLATEURL_SCHEME);

	if (lwc_string_caseless_isequal(scheme, corestring_lwc_file,
					&match) != lwc_error_ok)
	{
		return SLATEERROR_BAD_PARAMETER;
	}
	lwc_string_unref(scheme);
	if (match == false) {
		return SLATEERROR_BAD_PARAMETER;
	}

	urlpath = slateurl_get_component(url, SLATEURL_PATH);
	if (urlpath == NULL) {
		return SLATEERROR_BAD_PARAMETER;
	}

	res = url_unescape(lwc_string_data(urlpath),
			   lwc_string_length(urlpath),
			   NULL,
			   &path);
	lwc_string_unref(urlpath);
	if (res != SLATEERROR_OK) {
		return res;
	}

	*path_out = path;

	return SLATEERROR_OK;
}

/**
 * Create a slateurl from a path using posix file handling.
 *
 * Perform the necessary operations on a path to generate a slateurl.
 *
 * @param[in] path The path to convert.
 * @param[out] url_out pointer to recive the slateurl, The returned url
 *                     should be unreferenced by the caller.
 * @return SLATEERROR_OK and the url is placed in \a url or error code on
 *         faliure.
 */
static slateerror posix_path_to_slateurl(const char *path, struct slateurl **url_out)
{
	slateerror ret;
	int urllen;
	char *urlstr;
	char *escpath; /* escaped version of the path */
	char *escpaths;

	if ((path == NULL) || (url_out == NULL) || (*path == 0)) {
		return SLATEERROR_BAD_PARAMETER;
	}

	/* escape the path so it can be placed in a url */
	ret = url_escape(path, false, "/", &escpath);
	if (ret != SLATEERROR_OK) {
		return ret;
	}
	/* remove unecessary / as file: paths are already absolute */
	escpaths = escpath;
	while (*escpaths == '/') {
		escpaths++;
	}

	/* build url as a string for slateurl constructor */
	urllen = strlen(escpaths) + FILE_SCHEME_PREFIX_LEN + 1;
	urlstr = malloc(urllen);
	if (urlstr == NULL) {
		free(escpath);
		return SLATEERROR_NOMEM;
	}

	snprintf(urlstr, urllen, "%s%s", FILE_SCHEME_PREFIX, escpaths);
	free(escpath);

	ret = slateurl_create(urlstr, url_out);
	free(urlstr);

	return ret;
}

/**
 * Ensure that all directory elements needed to store a filename exist.
 *
 * @param fname The filename to ensure the path to exists.
 * @return SLATEERROR_OK on success or error code on failure.
 */
static slateerror posix_mkdir_all(const char *fname)
{
	char *dname;
	char *sep;
	struct stat sb;

	dname = strdup(fname);

	sep = strrchr(dname, '/');
	if (sep == NULL) {
		/* no directory separator path is just filename so its ok */
		free(dname);
		return SLATEERROR_OK;
	}

	*sep = 0; /* null terminate directory path */

	if (stat(dname, &sb) == 0) {
		free(dname);
		if (S_ISDIR(sb.st_mode)) {
			/* path to file exists and is a directory */
			return SLATEERROR_OK;
		}
		return SLATEERROR_NOT_DIRECTORY;
	}
	*sep = '/'; /* restore separator */

	sep = dname;
	while (*sep == '/') {
		sep++;
	}
	while ((sep = strchr(sep, '/')) != NULL) {
		*sep = 0;
		if (stat(dname, &sb) != 0) {
			if (slatemkdir(dname, S_IRWXU) != 0) {
				/* could not create path element */
				free(dname);
				return SLATEERROR_NOT_FOUND;
			}
		} else {
			if (! S_ISDIR(sb.st_mode)) {
				/* path element not a directory */
				free(dname);
				return SLATEERROR_NOT_DIRECTORY;
			}
		}
		*sep = '/'; /* restore separator */
		/* skip directory separators */
		while (*sep == '/') {
			sep++;
		}
	}

	free(dname);
	return SLATEERROR_OK;
}

/**
 * default to using the posix file handling
 */
static struct gui_file_table file_table = {
	.mkpath = posix_vmkpath,
	.basename = posix_basename,
	.slateurl_to_path = posix_slateurl_to_path,
	.path_to_slateurl = posix_path_to_slateurl,
	.mkdir_all = posix_mkdir_all,
};

struct gui_file_table *default_file_table = &file_table;

/* exported interface documented in utils/file.h */
slateerror slate_mkpath(char **str, size_t *size, size_t nelm, ...)
{
	va_list ap;
	slateerror ret;

	va_start(ap, nelm);
	ret = guit->file->mkpath(str, size, nelm, ap);
	va_end(ap);

	return ret;
}

/* exported interface documented in utils/file.h */
slateerror slate_slateurl_to_path(struct slateurl *url, char **path_out)
{
	return guit->file->slateurl_to_path(url, path_out);
}

/* exported interface documented in utils/file.h */
slateerror slate_path_to_slateurl(const char *path, struct slateurl **url)
{
	return guit->file->path_to_slateurl(path, url);
}

/* exported interface documented in utils/file.h */
slateerror slate_mkdir_all(const char *fname)
{
	return guit->file->mkdir_all(fname);
}

/* exported interface documented in utils/file.h */
slateerror
slate_recursive_rm(const char *path)
{
	DIR *parent;
	struct dirent *entry;
	slateerror ret = SLATEERROR_OK;
	struct stat ent_stat; /* stat result of leaf entry */

	parent = opendir(path);
	if (parent == NULL) {
		switch (errno) {
		case ENOENT:
			return SLATEERROR_NOT_FOUND;
		default:
			return SLATEERROR_UNKNOWN;
		}
	}

	while ((entry = readdir(parent))) {
		char *leafpath = NULL;

		if (strcmp(entry->d_name, ".") == 0 ||
		    strcmp(entry->d_name, "..") == 0)
			continue;

		ret = slate_mkpath(&leafpath, NULL, 2, path, entry->d_name);
		if (ret != SLATEERROR_OK)
			goto out;

#if (defined(HAVE_DIRFD) && defined(HAVE_FSTATAT))
		if (fstatat(dirfd(parent), entry->d_name, &ent_stat,
				AT_SYMLINK_NOFOLLOW) != 0) {
#else
		if (stat(leafpath, &ent_stat) != 0) {
#endif
			free(leafpath);
			goto out_via_errno;
		}
		if (S_ISDIR(ent_stat.st_mode)) {
			ret = slate_recursive_rm(leafpath);
			if (ret != SLATEERROR_OK) {
				free(leafpath);
				goto out;
			}
		} else {
#if (defined(HAVE_DIRFD) && defined(HAVE_UNLINKAT))
			if (unlinkat(dirfd(parent), entry->d_name, 0) != 0) {
#else
			if (unlink(leafpath) != 0) {
#endif
				free(leafpath);
				goto out_via_errno;
			}
		}

		free(leafpath);
	}

	if (rmdir(path) != 0) {
		goto out_via_errno;
	}

	goto out;

out_via_errno:
	switch (errno) {
	case ENOENT:
		ret = SLATEERROR_NOT_FOUND;
		break;
	default:
		ret = SLATEERROR_UNKNOWN;
	}
out:
	closedir(parent);

	return ret;
}
