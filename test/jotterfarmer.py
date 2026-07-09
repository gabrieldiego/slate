# Copyright 2017-2019 Daniel Silverstone <dsilvers@digital-scurf.org>
#
# This file is part of NetSurf, http://www.slate-browser.org/
#
# NetSurf is free software; you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation; version 2 of the License.
#
# NetSurf is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

"""
Jotter Farmer

The jotter farmer is a wrapper around `jotter` which can be used to simplify
access to the jotter behaviours and ultimately to write useful tests in an
expressive but not overcomplicated DSLish way.  Tests are, ultimately, still
Python code.

"""

# pylint: disable=locally-disabled, missing-docstring

import os
import selectors
import socket
import subprocess
import time
import sys

class StderrEcho:
    def __init__(self, sockend):
        self.sock = sockend
        self.sock.setblocking(False)
        self.incoming = b""
        self.lines = []
        self.js_diagnostics = []
        self.closed = False

    def close(self):
        self.closed = True
        if self.sock.fileno() != -1:
            self.sock.close()

    def handle_read(self):
        try:
            got = self.sock.recv(8192)
            if not got:
                self.closed = True
                return
        except BlockingIOError:
            return

        self.incoming += got
        if b"\n" in self.incoming:
            lines = self.incoming.split(b"\n")
            self.incoming = lines.pop()
            for line in lines:
                try:
                    line = line.decode('utf-8')
                except UnicodeDecodeError:
                    print("WARNING: Unicode decode error")
                    line = line.decode('utf-8', 'replace')

                self.lines.append(line)
                if self._is_js_diagnostic(line):
                    self.js_diagnostics.append(line)
                sys.stderr.write("{}\n".format(line))

    @staticmethod
    def _is_js_diagnostic(line):
        needles = (
            "jserrors",
            "Uncaught error in JS",
            "SyntaxError",
            "TypeError",
            "ReferenceError",
            "RangeError",
            "slatejs_dump_error",
        )
        return any(needle in line for needle in needles)


class JotterFarmer:

    # pylint: disable=locally-disabled, too-many-instance-attributes

    def __init__(self, jotter_cmd, jotter_env, online, quiet=False, *, wrapper=None):
        (mine, jotters) = socket.socketpair()
        (mine2, jotterserr) = socket.socketpair()

        self.sock = mine
        self.sock.setblocking(False)
        self._errwrapper = StderrEcho(mine2)
        self.selector = selectors.DefaultSelector()
        self._jotter_events = selectors.EVENT_READ

        if wrapper is not None:
            new_cmd = list(wrapper)
            new_cmd.extend(jotter_cmd)
            jotter_cmd = new_cmd

        self.jotter = subprocess.Popen(
            jotter_cmd,
            env=jotter_env,
            stdin=jotters,
            stdout=jotters,
            stderr=jotterserr,
            close_fds=True)

        jotters.close()
        jotterserr.close()
        self.selector.register(self.sock, self._jotter_events, "jotter")
        self.selector.register(self._errwrapper.sock, selectors.EVENT_READ, "stderr")

        self.buffer = b""
        self.incoming = b""
        self.lines = []
        self.scheduled = []
        self.deadjotter = False
        self.online = online
        self.quiet = quiet
        self.discussion = []
        self.maybe_slower = wrapper is not None

    def _set_jotter_events(self):
        if self.deadjotter:
            return
        events = selectors.EVENT_READ
        if len(self.buffer) > 0:
            events |= selectors.EVENT_WRITE
        if events != self._jotter_events:
            self._jotter_events = events
            self.selector.modify(self.sock, events, "jotter")

    def _close_stderr(self):
        if self._errwrapper.sock.fileno() == -1:
            return
        try:
            self.selector.unregister(self._errwrapper.sock)
        except (KeyError, ValueError):
            pass
        self._errwrapper.close()

    def _jotter_exited(self):
        if self.deadjotter:
            return
        self.deadjotter = True
        try:
            self.selector.unregister(self.sock)
        except (KeyError, ValueError):
            pass
        self.sock.close()
        # ensure the child process is finished and report the exit
        if self.jotter.poll() is None:
            self.jotter.terminate()
            self.jotter.wait()
        print("Handling an exit {}".format(self.jotter.returncode))
        print("The following are present in the queue: {}".format(self.lines))
        self.lines.append("GENERIC EXIT {}".format(
            self.jotter.returncode).encode('utf-8'))
        print("The queue is now: {}".format(self.lines))
        self._close_stderr()
        self.selector.close()

    def handle_read(self):
        try:
            got = self.sock.recv(8192)
            if not got:
                self._jotter_exited()
                return
        except BlockingIOError:
            return

        self.incoming += got
        if b"\n" in self.incoming:
            lines = self.incoming.split(b"\n")
            self.incoming = lines.pop()
            self.lines.extend(lines)

    def handle_write(self):
        if len(self.buffer) == 0:
            self._set_jotter_events()
            return
        try:
            sent = self.sock.send(self.buffer)
        except BlockingIOError:
            return
        except BrokenPipeError:
            self._jotter_exited()
            return
        if sent == 0:
            self._jotter_exited()
            return
        self.buffer = self.buffer[sent:]
        self._set_jotter_events()

    def tell_jotter(self, *args):
        cmd = (" ".join(args))
        if not self.quiet:
            print(">>> {}".format(cmd))
        self.discussion.append((">", cmd))
        cmd = cmd + "\n"
        self.buffer += cmd.encode('utf-8')
        self._set_jotter_events()

    def report_js_diagnostics(self):
        diagnostics = self._errwrapper.js_diagnostics
        if not diagnostics:
            print("JS diagnostics: none")
            return

        print("JS diagnostics: {} warning/error lines".format(len(diagnostics)))
        for line in diagnostics[:40]:
            print("JS diagnostic: {}".format(line))
        if len(diagnostics) > 40:
            print("JS diagnostic: ... {} more lines".format(len(diagnostics) - 40))

    def jotter_says(self, line):
        try:
            line = line.decode('utf-8')
        except UnicodeDecodeError:
            print("WARNING: Unicode decode error")
            line = line.decode('utf-8', 'replace')
        if not self.quiet:
            print("<<< {}".format(line))
        self.discussion.append(("<", line))
        self.online(line)

    def schedule_event(self, event, secs=None, when=None):
        assert secs is not None or when is not None
        if when is None:
            when = time.time() + secs
        self.scheduled.append((when, event))
        self.scheduled.sort()

    def unschedule_event(self, event):
        self.scheduled = [x for x in self.scheduled if x[1] != event]

    def _handle_events(self, events):
        for key, mask in events:
            if key.data == "jotter":
                if mask & selectors.EVENT_READ:
                    self.handle_read()
                if not self.deadjotter and mask & selectors.EVENT_WRITE:
                    self.handle_write()
            elif key.data == "stderr":
                if mask & selectors.EVENT_READ:
                    self._errwrapper.handle_read()
                if self._errwrapper.closed:
                    self._close_stderr()

    def _deliver_pending_lines(self, once):
        while len(self.lines) > 0:
            self.jotter_says(self.lines.pop(0))
            if once or self.deadjotter:
                return True
        return False

    def loop(self, once=False):
        if self._deliver_pending_lines(once):
            return
        while True:
            if self.deadjotter:
                return
            now = time.time()
            while len(self.scheduled) > 0 and now >= self.scheduled[0][0]:
                func = self.scheduled[0][1]
                self.scheduled.pop(0)
                func(self)
                now = time.time()
            if len(self.scheduled) > 0:
                timeout = max(0, self.scheduled[0][0] - now)
            else:
                timeout = None
            try:
                events = self.selector.select(timeout)
            except OSError:
                if self.deadjotter:
                    return
                raise
            self._handle_events(events)
            if self._deliver_pending_lines(once):
                return


class Browser:

    # pylint: disable=locally-disabled, too-many-instance-attributes, dangerous-default-value, invalid-name

    def __init__(self, jotter_cmd=["./jotter"], jotter_env=None, quiet=False, *, wrapper=None):
        self.farmer = JotterFarmer(
            jotter_cmd=jotter_cmd,
            jotter_env=jotter_env,
            online=self.on_jotter_line,
            quiet=quiet,
            wrapper=wrapper)
        self.windows = {}
        self.logins = {}
        self.current_draw_target = None
        self.started = False
        self.stopped = False
        self.launchurl = None
        now = time.time()
        timeout = now + 1

        if wrapper is not None:
            timeout = now + 10

        while not self.started:
            self.farmer.loop(once=True)
            if time.time() > timeout:
                break

    def pass_options(self, *opts):
        if len(opts) > 0:
            self.farmer.tell_jotter("OPTIONS " + (" ".join(['--' + opt for opt in opts])))

    def on_jotter_line(self, line):
        parts = line.split(" ")
        handler = getattr(self, "handle_" + parts[0], None)
        if handler is not None:
            handler(*parts[1:])

    def quit(self):
        self.farmer.tell_jotter("QUIT")

    def quit_and_wait(self):
        self.quit()
        self.farmer.loop()
        return self.stopped

    def handle_GENERIC(self, what, *args):
        if what == 'STARTED':
            self.started = True
        elif what == 'FINISHED':
            self.stopped = True
        elif what == 'LAUNCH':
            self.launchurl = args[1]
        elif what == 'EXIT':
            if not self.stopped:
                print("Unexpected exit of jotter process with code {}".format(args[0]))
            assert self.stopped
        else:
            pass

    def handle_WINDOW(self, action, _win, winid, *args):
        if action == "NEW":
            new_win = BrowserWindow(self, winid, *args)
            self.windows[winid] = new_win
        else:
            win = self.windows.get(winid, None)
            if win is None:
                print("    Unknown window id {}".format(winid))
            else:
                win.handle(action, *args)

    def handle_LOGIN(self, action, _lwin, winid, *args):
        if action == "OPEN":
            new_win = LoginWindow(self, winid, *args)
            self.logins[winid] = new_win
        else:
            win = self.logins.get(winid, None)
            if win is None:
                print("    Unknown login window id {}".format(winid))
            else:
                win.handle(action, *args)
                if win.alive and win.ready:
                    self.handle_ready_login(win)

    def handle_PLOT(self, *args):
        if self.current_draw_target is not None:
            self.current_draw_target.handle_plot(*args)

    def new_window(self, url=None):
        if url is None:
            self.farmer.tell_jotter("WINDOW NEW")
        else:
            self.farmer.tell_jotter("WINDOW NEW %s" % url)
        wins_known = set(self.windows.keys())
        while len(set(self.windows.keys()).difference(wins_known)) == 0:
            self.farmer.loop(once=True)
        poss_wins = set(self.windows.keys()).difference(wins_known)
        return self.windows[poss_wins.pop()]

    def handle_ready_login(self, lwin):

        # pylint: disable=locally-disabled, no-self-use

        # Override this method to do useful stuff
        lwin.destroy()


class LoginWindow:

    # pylint: disable=locally-disabled, too-many-instance-attributes, invalid-name

    def __init__(self, browser, winid, _url, *url):
        self.alive = True
        self.ready = False
        self.browser = browser
        self.winid = winid
        self.url = " ".join(url)
        self.username = None
        self.password = None
        self.realm = None

    def handle(self, action, _str="STR", *rest):
        content = " ".join(rest)
        if action == "USER":
            self.username = content
        elif action == "PASS":
            self.password = content
        elif action == "REALM":
            self.realm = content
        elif action == "DESTROY":
            self.alive = False
        else:
            raise AssertionError("Unknown action {} for login window".format(action))
        if not (self.username is None or self.password is None or self.realm is None):
            self.ready = True

    def send_username(self, username=None):
        assert self.alive
        if username is None:
            username = self.username
        self.browser.farmer.tell_jotter("LOGIN USERNAME {} {}".format(self.winid, username))

    def send_password(self, password=None):
        assert self.alive
        if password is None:
            password = self.password
        self.browser.farmer.tell_jotter("LOGIN PASSWORD {} {}".format(self.winid, password))

    def _wait_dead(self):
        while self.alive:
            self.browser.farmer.loop(once=True)

    def go(self):
        assert self.alive
        self.browser.farmer.tell_jotter("LOGIN GO {}".format(self.winid))
        self._wait_dead()

    def destroy(self):
        assert self.alive
        self.browser.farmer.tell_jotter("LOGIN DESTROY {}".format(self.winid))
        self._wait_dead()


class BrowserWindow:

    # pylint: disable=locally-disabled, too-many-instance-attributes, too-many-public-methods, invalid-name

    def __init__(
            self,
            browser,
            winid,
            _for,
            coreid,
            _existing,
            otherid,
            _newtab,
            newtab,
            _clone,
            clone):
        # pylint: disable=locally-disabled, too-many-arguments
        self.alive = True
        self.browser = browser
        self.winid = winid
        self.coreid = coreid
        self.existing = browser.windows.get(otherid, None)
        self.newtab = newtab == "TRUE"
        self.clone = clone == "TRUE"
        self.width = 0
        self.height = 0
        self.title = ""
        self.throbbing = False
        self.scrollx = 0
        self.scrolly = 0
        self.content_width = 0
        self.content_height = 0
        self.status = ""
        self.pointer = ""
        self.scale = 1.0
        self.url = ""
        self.plotted = []
        self.plotting = False
        self.log_entries = []
        self.page_info_state = "UNKNOWN"

    def kill(self):
        self.browser.farmer.tell_jotter("WINDOW DESTROY %s" % self.winid)

    def wait_until_dead(self, timeout=1):
        now = time.time()
        while self.alive:
            self.browser.farmer.loop(once=True)
            if (time.time() - now) > timeout:
                print("*** Timed out waiting for window to be destroyed")
                print("*** URL was: {}".format(self.url))
                print("*** Title was: {}".format(self.title))
                print("*** Status was: {}".format(self.status))
                break

    def go(self, url, referer=None):
        if referer is None:
            self.browser.farmer.tell_jotter("WINDOW GO %s %s" % (
                self.winid, url))
        else:
            self.browser.farmer.tell_jotter("WINDOW GO %s %s %s" % (
                self.winid, url, referer))
        self.wait_start_loading()

    def stop(self):
        self.browser.farmer.tell_jotter("WINDOW STOP %s" % (self.winid))

    def reload(self, all=False):
        all = " ALL" if all else ""
        self.browser.farmer.tell_jotter("WINDOW RELOAD %s%s" % (self.winid, all))
        self.wait_start_loading()

    def click(self, x, y, button="LEFT", kind="SINGLE"):
        self.browser.farmer.tell_jotter("WINDOW CLICK WIN %s X %s Y %s BUTTON %s KIND %s" % (self.winid, x, y, button, kind))

    def key(self, key=None, value=None, text=None):
        assert (key is not None) + (value is not None) + (text is not None) == 1
        if key is not None:
            key = str(key).upper().replace("-", "_")
            self.browser.farmer.tell_jotter("WINDOW KEY WIN %s NAME %s" % (self.winid, key))
        elif value is not None:
            self.browser.farmer.tell_jotter("WINDOW KEY WIN %s VALUE %s" % (self.winid, value))
        else:
            text = str(text)
            assert "\n" not in text, "Jotter key text cannot contain newlines"
            self.browser.farmer.tell_jotter("WINDOW KEY WIN %s TEXT %s" % (self.winid, text))

    def mouse_track(self, x, y, state="HOVER"):
        if isinstance(state, (list, tuple)):
            state = "+".join(str(part).upper().replace("-", "_") for part in state)
        else:
            state = str(state).upper().replace("-", "_")
        self.browser.farmer.tell_jotter("WINDOW MOUSE WIN %s X %s Y %s STATE %s" % (self.winid, x, y, state))

    def mouse_click(self, x, y, state="CLICK_1"):
        if isinstance(state, (list, tuple)):
            state = "+".join(str(part).upper().replace("-", "_") for part in state)
        else:
            state = str(state).upper().replace("-", "_")
        self.browser.farmer.tell_jotter("WINDOW MOUSECLICK WIN %s X %s Y %s STATE %s" % (self.winid, x, y, state))

    def scroll(self, x, y, dx=0, dy=0):
        self.browser.farmer.tell_jotter("WINDOW SCROLL WIN %s X %s Y %s DX %s DY %s" % (self.winid, x, y, dx, dy))

    def js_exec(self, src):
        self.browser.farmer.tell_jotter("WINDOW EXEC WIN %s %s" % (self.winid, src))

    def handle(self, action, *args):
        handler = getattr(self, "handle_window_" + action, None)
        if handler is not None:
            handler(*args)

    def handle_window_SIZE(self, _width, width, _height, height):
        self.width = int(width)
        self.height = int(height)

    def handle_window_DESTROY(self):
        self.alive = False

    def handle_window_TITLE(self, _str, *title):
        self.title = " ".join(title)

    def handle_window_GET_DIMENSIONS(self, _width, width, _height, height):
        self.width = width
        self.height = height

    def handle_window_NEW_CONTENT(self):
        pass

    def handle_window_NEW_ICON(self):
        pass

    def handle_window_START_THROBBER(self):
        self.throbbing = True

    def handle_window_STOP_THROBBER(self):
        self.throbbing = False

    def handle_window_SET_SCROLL(self, _x, x, _y, y):
        self.scrollx = int(x)
        self.scrolly = int(y)

    def handle_window_UPDATE_BOX(self, _x, x, _y, y, _width, width, _height, height):
        # pylint: disable=locally-disabled, no-self-use

        x = int(x)
        y = int(y)
        width = int(width)
        height = int(height)

    def handle_window_UPDATE_EXTENT(self, _width, width, _height, height):
        self.content_width = int(width)
        self.content_height = int(height)

    def handle_window_SET_STATUS(self, _str, *status):
        self.status = (" ".join(status))

    def handle_window_SET_POINTER(self, _ptr, ptr):
        self.pointer = ptr

    def handle_window_SET_SCALE(self, _scale, scale):
        self.scale = float(scale)

    def handle_window_SET_URL(self, _url, url):
        self.url = url

    def handle_window_GET_SCROLL(self, _x, x, _y, y):
        self.scrollx = int(x)
        self.scrolly = int(y)

    def handle_window_SCROLL_START(self, *args):
        self.scrollx = 0
        self.scrolly = 0

    def handle_window_REDRAW(self, act):
        if act == "START":
            self.browser.current_draw_target = self
            self.plotted = []
            self.plotting = True
        else:
            self.browser.current_draw_target = None
            self.plotting = False

    def handle_window_CONSOLE_LOG(self, _src, src, folding, level, *msg):
        self.log_entries.append((src, folding == "FOLDABLE", level, " ".join(msg)))

    def handle_window_PAGE_STATUS(self, _status, status):
        self.page_info_state = status

    def load_page(self, url=None, referer=None):
        if url is not None:
            self.go(url, referer)
        self.wait_loaded()

    def wait_start_loading(self):
        while not self.throbbing:
            self.browser.farmer.loop(once=True)

    def wait_loaded(self):
        self.wait_start_loading()
        while self.throbbing:
            self.browser.farmer.loop(once=True)

    def handle_plot(self, *args):
        self.plotted.append(args)

    def redraw(self, coords=None):
        if coords is None:
            self.browser.farmer.tell_jotter("WINDOW REDRAW %s" % self.winid)
        else:
            self.browser.farmer.tell_jotter("WINDOW REDRAW %s %s" % (
                self.winid, (" ".join(coords))))
        while not self.plotting:
            self.browser.farmer.loop(once=True)
        while self.plotting:
            self.browser.farmer.loop(once=True)
        return self.plotted

    def clear_log(self):
        self.log_entries = []

    def log_contains(self, source=None, foldable=None, level=None, substr=None):
        if (source is None) and (foldable is None) and (level is None) and (substr is None):
            assert False, "Unable to run log_contains, no predicate given"

        for (source_, foldable_, level_, msg_) in self.log_entries:
            ok = True
            if (source is not None) and (source != source_):
                ok = False
            if (foldable is not None) and (foldable != foldable_):
                ok = False
            if (level is not None) and (level != level_):
                ok = False
            if (substr is not None) and (substr not in msg_):
                ok = False
            if ok:
                return True

        return False

    def wait_for_log(self, source=None, foldable=None, level=None, substr=None, timeout=None):
        start = time.time()
        while not self.log_contains(source=source, foldable=foldable, level=level, substr=substr):
            if timeout is not None and time.time() - start > timeout:
                recent = [entry[3] for entry in self.log_entries[-8:]]
                raise AssertionError(
                    "Timed out waiting for log substring {!r}; recent logs: {}".format(
                        substr, recent))
            self.browser.farmer.loop(once=True)


def farmer_test():
    '''
    Simple farmer test
    '''

    browser = Browser(quiet=True)
    win = browser.new_window()

    fname = "test/js/inline-doc-write-simple.html"
    full_fname = os.path.join(os.getcwd(), fname)

    browser.pass_options("--enable_javascript=0")
    win.load_page("file://" + full_fname)

    print("Loaded, URL is {}".format(win.url))

    cmds = win.redraw()
    print("Received {} plot commands".format(len(cmds)))
    for cmd in cmds:
        if cmd[0] == "TEXT":
            text_x = cmd[2]
            text_y = cmd[4]
            rest = " ".join(cmd[6:])
            print("{} {} -> {}".format(text_x, text_y, rest))

    browser.pass_options("--enable_javascript=1")
    win.load_page("file://" + full_fname)

    print("Loaded, URL is {}".format(win.url))

    cmds = win.redraw()
    print("Received {} plot commands".format(len(cmds)))
    for cmd in cmds:
        if cmd[0] == "TEXT":
            text_x = cmd[2]
            text_y = cmd[4]
            rest = " ".join(cmd[6:])
            print("{} {} -> {}".format(text_x, text_y, rest))

    browser.quit_and_wait()

    class FooBarLogin(Browser):
        def handle_ready_login(self, lwin):
            lwin.send_username("foo")
            lwin.send_password("bar")
            lwin.go()

    fbbrowser = FooBarLogin(quiet=True)
    win = fbbrowser.new_window()
    win.load_page("https://httpbin.org/basic-auth/foo/bar")
    cmds = win.redraw()
    print("Received {} plot commands for auth test".format(len(cmds)))
    for cmd in cmds:
        if cmd[0] == "TEXT":
            text_x = cmd[2]
            text_y = cmd[4]
            rest = " ".join(cmd[6:])
            print("{} {} -> {}".format(text_x, text_y, rest))

    fname = "test/js/inserted-script.html"
    full_fname = os.path.join(os.getcwd(), fname)

    browser = Browser(quiet=True)
    browser.pass_options("--enable_javascript=1")
    win = browser.new_window()
    win.load_page("file://" + full_fname)
    print("Loaded, URL is {}".format(win.url))

    win.wait_for_log(substr="deferred")

    # print("Discussion was:")
    # for line in browser.farmer.discussion:
    #    print("{} {}".format(line[0], line[1]))


if __name__ == '__main__':
    farmer_test()
