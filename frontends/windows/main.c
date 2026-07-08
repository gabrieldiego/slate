/*
 * Copyright 2011 Vincent Sanders <vince@simtec.co.uk>
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

#include "utils/config.h"

#include <limits.h>
#include <stdbool.h>
#include <windows.h>
#include <shlobj.h>
#include <shlwapi.h>
#include <io.h>

#include "utils/utils.h"
#include "utils/log.h"
#include "utils/messages.h"
#include "utils/filepath.h"
#include "utils/file.h"
#include "utils/slateurl.h"
#include "utils/slateoption.h"
#include "slate/url_db.h"
#include "slate/cookie_db.h"
#include "slate/browser.h"
#include "slate/browser_window.h"
#include "slate/fetch.h"
#include "slate/misc.h"
#include "slate/slate.h"
#include "desktop/hotlist.h"

#include "windows/findfile.h"
#include "windows/file.h"
#include "windows/cookies.h"
#include "windows/drawable.h"
#include "windows/corewindow.h"
#include "windows/download.h"
#include "windows/local_history.h"
#include "windows/window.h"
#include "windows/schedule.h"
#include "windows/font.h"
#include "windows/fetch.h"
#include "windows/pointers.h"
#include "windows/bitmap.h"
#include "windows/clipboard.h"
#include "windows/gui.h"


/**
 * Obtain the DPI of the display.
 *
 * \return The DPI of the device the window is displayed on.
 */
static int get_screen_dpi(void)
{
	HDC screendc = GetDC(0);
	int dpi = GetDeviceCaps(screendc, LOGPIXELSY);
	ReleaseDC(0, screendc);

	if (dpi <= 10) {
		dpi = 96; /* 96DPI is the default */
	}

	NSLOG(netsurf, INFO, "FIX DPI %d", dpi);

	return dpi;
}

/**
 * Get the path to the config directory.
 *
 * This ought to use SHGetKnownFolderPath(FOLDERID_RoamingAppData) and
 * PathCcpAppend() but uses depricated API because that is what mingw
 * supports.
 *
 * @param config_home_out Path to configuration directory.
 * @return SLATEERROR_OK on sucess and \a config_home_out updated else error code.
 */
static slateerror get_config_home(char **config_home_out)
{
	TCHAR adPath[MAX_PATH]; /* appdata path */
	char nsdir[] = "NetSurf";
	HRESULT hres;

	hres = SHGetFolderPath(NULL,
			       CSIDL_APPDATA | CSIDL_FLAG_CREATE,
			       NULL,
			       SHGFP_TYPE_CURRENT,
			       adPath);
	if (hres != S_OK) {
		return SLATEERROR_INVALID;
	}

	if (PathAppend(adPath, nsdir) == false) {
		return SLATEERROR_NOT_FOUND;
	}

	/* ensure netsurf directory exists */
	if (CreateDirectory(adPath, NULL) == 0) {
		DWORD dw;
		dw = GetLastError();
		if (dw != ERROR_ALREADY_EXISTS) {
			return SLATEERROR_NOT_DIRECTORY;
		}
	}

	*config_home_out = strdup(adPath);

	NSLOG(netsurf, INFO, "using config path \"%s\"", *config_home_out);

	return SLATEERROR_OK;
}


/**
 * Cause an abnormal program termination.
 *
 * \note This never returns and is intended to terminate without any cleanup.
 *
 * \param error The message to display to the user.
 */
static void die(const char *error)
{
	exit(1);
}



/**
 * Ensures output logging stream is available
 */
static bool nslog_ensure(FILE *fptr)
{
	/* mwindows compile flag normally invalidates standard io unless
	 *  already redirected
	 */
	if (_get_osfhandle(fileno(fptr)) == -1) {
		AllocConsole();
		freopen("CONOUT$", "w", fptr);
	}
	return true;
}

/**
 * Set option defaults for windows frontend
 *
 * @param defaults The option table to update.
 * @return error status.
 */
static slateerror set_defaults(struct slateoption_s *defaults)
{
	/* Set defaults for absent option strings */

	/* locate CA bundle and set as default, cannot rely on curl
	 * compiled in default on windows.
	 */
	DWORD res_len;
	DWORD buf_tchar_size = PATH_MAX + 1;
	DWORD buf_bytes_size = sizeof(TCHAR) * buf_tchar_size;
	char *ptr = NULL;
	char *buf;
	char *fname;
	HRESULT hres;
	char dldir[] = "Downloads";

	buf = malloc(buf_bytes_size);
	if (buf== NULL) {
		return SLATEERROR_NOMEM;
	}
	buf[0] = '\0';

	/* locate certificate bundle */
	res_len = SearchPathA(NULL,
			      "ca-bundle.crt",
			      NULL,
			      buf_tchar_size,
			      buf,
			      &ptr);
	if (res_len > 0) {
		slateoption_setnull_charp(ca_bundle, strdup(buf));
	} else {
		ptr = filepath_sfind(G_resource_pathv, buf, "ca-bundle.crt");
		if (ptr != NULL) {
			slateoption_setnull_charp(ca_bundle, strdup(buf));
		}
	}


	/* download directory default
	 *
	 * unfortunately SHGetKnownFolderPath(FOLDERID_Downloads) is
	 * not available so use the obsolete method of user prodile
	 * with downloads suffixed
	 */
	buf[0] = '\0';

	hres = SHGetFolderPath(NULL,
			       CSIDL_PROFILE | CSIDL_FLAG_CREATE,
			       NULL,
			       SHGFP_TYPE_CURRENT,
			       buf);
	if (hres == S_OK) {
		if (PathAppend(buf, dldir)) {
			slateoption_setnull_charp(downloads_directory,
					       strdup(buf));

		}
	}

	free(buf);

	/* ensure homepage option has a default */
	slateoption_setnull_charp(homepage_url, strdup(SLATE_HOMEPAGE));

	/* cookie file default */
	fname = NULL;
	slate_mkpath(&fname, NULL, 2, G_config_path, "Cookies");
	if (fname != NULL) {
		slateoption_setnull_charp(cookie_file, fname);
	}

	/* cookie jar default */
	fname = NULL;
	slate_mkpath(&fname, NULL, 2, G_config_path, "Cookies");
	if (fname != NULL) {
		slateoption_setnull_charp(cookie_jar, fname);
	}

	/* url database default */
	fname = NULL;
	slate_mkpath(&fname, NULL, 2, G_config_path, "URLs");
	if (fname != NULL) {
		slateoption_setnull_charp(url_file, fname);
	}

	/* bookmark database default */
	fname = NULL;
	slate_mkpath(&fname, NULL, 2, G_config_path, "Hotlist");
	if (fname != NULL) {
		slateoption_setnull_charp(hotlist_path, fname);
	}

	return SLATEERROR_OK;
}


/**
 * Initialise user options location and contents
 */
static slateerror
slatew32_option_init(int *pargc, char** argv, char **respaths, char *config_path)
{
	slateerror ret;
	char *choices = NULL;

	/* set the globals that will be used in the set_defaults() callback */
	G_resource_pathv = respaths;
	G_config_path = config_path;

	/* user options setup */
	ret = slateoption_init(set_defaults, &slateoptions, &slateoptions_default);
	if (ret != SLATEERROR_OK) {
		return ret;
	}

	/* Attempt to load the user choices */
	ret = slate_mkpath(&choices, NULL, 2, config_path, "Choices");
	if (ret == SLATEERROR_OK) {
		slateoption_read(choices, slateoptions);
		free(choices);
	}

	/* overide loaded options with those from commandline */
	slateoption_commandline(pargc, argv, slateoptions);

	return SLATEERROR_OK;
}

/**
 * Initialise messages
 */
static slateerror slatew32_messages_init(char **respaths)
{
	char *messages;
	slateerror res;
	const uint8_t *data;
	size_t data_size;

	res = slatew32_get_resource_data("messages", &data, &data_size);
	if (res == SLATEERROR_OK) {
		res = messages_add_from_inline(data, data_size);
	} else {
		/* Obtain path to messages */
		messages = filepath_find(respaths, "messages");
		if (messages == NULL) {
			res = SLATEERROR_NOT_FOUND;
		} else {
			res = messages_add_from_file(messages);
			free(messages);
		}
	}

	return res;
}


/**
 * Construct a unix style argc/argv
 *
 * \param argc_out number of commandline arguments
 * \param argv_out string vector of command line arguments
 * \return SLATEERROR_OK on success else error code
 */
static slateerror win32_to_unix_commandline(int *argc_out, char ***argv_out)
{
	int argc = 0;
	char **argv;
	int cura;
	LPWSTR *argvw;
	size_t len;

	argvw = CommandLineToArgvW(GetCommandLineW(), &argc);
	if (argvw == NULL) {
		return SLATEERROR_INVALID;
	}

	argv = malloc(sizeof(char *) * argc);
	if (argv == NULL) {
		return SLATEERROR_NOMEM;
	}

	for (cura = 0; cura < argc; cura++) {

		len = wcstombs(NULL, argvw[cura], 0) + 1;
		if (len > 0) {
			argv[cura] = malloc(len);
			if (argv[cura] == NULL) {
				free(argv);
				return SLATEERROR_NOMEM;
			}
		} else {
			free(argv);
			return SLATEERROR_INVALID;
		}

		wcstombs(argv[cura], argvw[cura], len);
		/* alter windows-style forward slash flags to hyphen flags. */
		if (argv[cura][0] == '/') {
			argv[cura][0] = '-';
		}
	}

	*argc_out = argc;
	*argv_out = argv;

	return SLATEERROR_OK;
}


static struct gui_misc_table win32_misc_table = {
	.schedule = win32_schedule,
	.present_cookies = slatew32_cookies_present,
};

/**
 * Entry point from windows
 **/
int WINAPI
WinMain(HINSTANCE hInstance, HINSTANCE hLastInstance, LPSTR lpcli, int ncmd)
{
	int argc;
	char **argv;
	char **respaths;
	char *slatew32_config_home = NULL;
	slateerror ret;
	const char *addr;
	slateurl *url;
	struct slate_table win32_table = {
		.misc = &win32_misc_table,
		.window = win32_window_table,
		.corewindow = win32_core_window_table,
		.clipboard = win32_clipboard_table,
		.download = win32_download_table,
		.fetch = win32_fetch_table,
		.file = win32_file_table,
		.utf8 = win32_utf8_table,
		.bitmap = win32_bitmap_table,
		.layout = win32_layout_table,
	};

	ret = slate_register(&win32_table);
	if (ret != SLATEERROR_OK) {
		die("NetSurf operation table registration failed");
	}

	/* Save the application-instance handle. */
	hinst = hInstance;

	setbuf(stderr, NULL);

	ret = win32_to_unix_commandline(&argc, &argv);
	if (ret != SLATEERROR_OK) {
		/* no log as logging requires this for initialisation */
		return 1;
	}

	/* initialise logging - not fatal if it fails but not much we
	 * can do about it
	 */
	nslog_init(nslog_ensure, &argc, argv);

	/* build resource path string vector */
	respaths = nsws_init_resource("${APPDATA}\\NetSurf:${PROGRAMFILES}\\NetSurf\\NetSurf\\:"SLATE_WINDOWS_RESPATH);

	/* Locate the correct user configuration directory path */
	ret = get_config_home(&slatew32_config_home);
	if (ret != SLATEERROR_OK) {
		NSLOG(netsurf, INFO,
		      "Unable to locate a configuration directory.");
	}

	/* Initialise user options */
	ret = slatew32_option_init(&argc, argv, respaths, slatew32_config_home);
	if (ret != SLATEERROR_OK) {
		NSLOG(netsurf, ERROR, "Options failed to initialise (%s)\n",
		      messages_get_errorcode(ret));
		return 1;
	}

	/* Initialise translated messages */
	ret = slatew32_messages_init(respaths);
	if (ret != SLATEERROR_OK) {
		fprintf(stderr, "Unable to load translated messages (%s)\n",
			messages_get_errorcode(ret));
		NSLOG(netsurf, INFO, "Unable to load translated messages");
		/** \todo decide if message load faliure should be fatal */
	}

	/* common initialisation */
	ret = slate_init(NULL);
	if (ret != SLATEERROR_OK) {
		NSLOG(netsurf, INFO, "NetSurf failed to initialise");
		return 1;
	}

	browser_set_dpi(get_screen_dpi());

	urldb_load(slateoption_charp(url_file));
	urldb_load_cookies(slateoption_charp(cookie_file));
	hotlist_init(slateoption_charp(hotlist_path),
			slateoption_charp(hotlist_path));

	ret = nsws_create_main_class(hInstance);
	ret = nsws_create_drawable_class(hInstance);
	ret = slatew32_create_corewindow_class(hInstance);

	slateoption_set_bool(target_blank, false);

	nsws_window_init_pointers(hInstance);

	/* If there is a url specified on the command line use it */
	if (argc > 1) {
		addr = argv[1];
	} else if (slateoption_charp(homepage_url) != NULL) {
		addr = slateoption_charp(homepage_url);
	} else {
		addr = SLATE_HOMEPAGE;
	}

	NSLOG(netsurf, INFO, "calling browser_window_create");

	ret = slateurl_create(addr, &url);
	if (ret == SLATEERROR_OK) {
		ret = browser_window_create(BW_CREATE_HISTORY,
					      url,
					      NULL,
					      NULL,
					      NULL);
		slateurl_unref(url);

	}
	if (ret != SLATEERROR_OK) {
		win32_warning(messages_get_errorcode(ret), 0);
	} else {
		win32_run();
	}

	urldb_save_cookies(slateoption_charp(cookie_jar));
	urldb_save(slateoption_charp(url_file));

	slate_exit();

	/* finalise options */
	slateoption_finalise(slateoptions, slateoptions_default);

	/* finalise logging */
	nslog_finalise();

	return 0;
}
