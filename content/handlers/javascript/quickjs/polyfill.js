/* Polyfiller for QuickJS for NetSurf
 *
 * This JavaScript will be loaded into heaps before the generics
 *
 * We only care for the side-effects of this, be careful.
 */

// Production steps of ECMA-262, Edition 6, 22.1.2.1
if (!Array.from) {
  Array.from = (function () {
    var toStr = Object.prototype.toString;
    var isCallable = function (fn) {
      return typeof fn === 'function' || toStr.call(fn) === '[object Function]';
    };
    var toInteger = function (value) {
      var number = Number(value);
      if (isNaN(number)) { return 0; }
      if (number === 0 || !isFinite(number)) { return number; }
      return (number > 0 ? 1 : -1) * Math.floor(Math.abs(number));
    };
    var maxSafeInteger = Math.pow(2, 53) - 1;
    var toLength = function (value) {
      var len = toInteger(value);
      return Math.min(Math.max(len, 0), maxSafeInteger);
    };

    // The length property of the from method is 1.
    return function from(arrayLike/*, mapFn, thisArg */) {
      // 1. Let C be the this value.
      var C = this;

      // 2. Let items be ToObject(arrayLike).
      var items = Object(arrayLike);

      // 3. ReturnIfAbrupt(items).
      if (arrayLike == null) {
        throw new TypeError('Array.from requires an array-like object - not null or undefined');
      }

      // 4. If mapfn is undefined, then let mapping be false.
      var mapFn = arguments.length > 1 ? arguments[1] : void undefined;
      var T;
      if (typeof mapFn !== 'undefined') {
        // 5. else
        // 5. a If IsCallable(mapfn) is false, throw a TypeError exception.
        if (!isCallable(mapFn)) {
          throw new TypeError('Array.from: when provided, the second argument must be a function');
        }

        // 5. b. If thisArg was supplied, let T be thisArg; else let T be undefined.
        if (arguments.length > 2) {
          T = arguments[2];
        }
      }

      // 10. Let lenValue be Get(items, "length").
      // 11. Let len be ToLength(lenValue).
      var len = toLength(items.length);

      // 13. If IsConstructor(C) is true, then
      // 13. a. Let A be the result of calling the [[Construct]] internal method 
      // of C with an argument list containing the single item len.
      // 14. a. Else, Let A be ArrayCreate(len).
      var A = isCallable(C) ? Object(new C(len)) : new Array(len);

      // 16. Let k be 0.
      var k = 0;
      // 17. Repeat, while k < len… (also steps a - h)
      var kValue;
      while (k < len) {
        kValue = items[k];
        if (mapFn) {
          A[k] = typeof T === 'undefined' ? mapFn(kValue, k) : mapFn.call(T, kValue, k);
        } else {
          A[k] = kValue;
        }
        k += 1;
      }
      // 18. Let putStatus be Put(A, "length", len, true).
      A.length = len;
      // 20. Return A.
      return A;
    };
  }());
}

if (typeof TextDecoder === "undefined") {
  (function () {
    var replacement = 0xfffd;

    function bytesFrom(input) {
      if (input == null) {
        return new Uint8Array(0);
      }
      if (ArrayBuffer.isView(input)) {
        return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
      }
      if (input instanceof ArrayBuffer) {
        return new Uint8Array(input);
      }
      throw new TypeError("TextDecoder.decode expects a buffer source");
    }

    function utf8Error(decoder) {
      if (decoder.fatal) {
        throw new TypeError("The encoded data is not valid UTF-8");
      }
      return replacement;
    }

    function appendCodePoint(state, cp) {
      if (cp <= 0xffff) {
        state.chunk += String.fromCharCode(cp);
      } else {
        cp -= 0x10000;
        state.chunk += String.fromCharCode(0xd800 + (cp >> 10),
            0xdc00 + (cp & 0x3ff));
      }
      if (state.chunk.length >= 8192) {
        state.parts.push(state.chunk);
        state.chunk = "";
      }
    }

    function decodeUtf8(decoder, bytes) {
      var state = { parts: [], chunk: "" };
      var i = 0;

      while (i < bytes.length) {
        var b1 = bytes[i++];
        var b2;
        var b3;
        var b4;
        var cp;

        if (b1 < 0x80) {
          appendCodePoint(state, b1);
        } else if (b1 >= 0xc2 && b1 <= 0xdf && i < bytes.length) {
          b2 = bytes[i];
          if ((b2 & 0xc0) === 0x80) {
            i++;
            appendCodePoint(state, ((b1 & 0x1f) << 6) | (b2 & 0x3f));
          } else {
            appendCodePoint(state, utf8Error(decoder));
          }
        } else if (b1 >= 0xe0 && b1 <= 0xef && i + 1 < bytes.length) {
          b2 = bytes[i];
          b3 = bytes[i + 1];
          if ((b2 & 0xc0) === 0x80 && (b3 & 0xc0) === 0x80 &&
              !(b1 === 0xe0 && b2 < 0xa0) &&
              !(b1 === 0xed && b2 >= 0xa0)) {
            i += 2;
            cp = ((b1 & 0x0f) << 12) | ((b2 & 0x3f) << 6) | (b3 & 0x3f);
            appendCodePoint(state, cp);
          } else {
            appendCodePoint(state, utf8Error(decoder));
          }
        } else if (b1 >= 0xf0 && b1 <= 0xf4 && i + 2 < bytes.length) {
          b2 = bytes[i];
          b3 = bytes[i + 1];
          b4 = bytes[i + 2];
          if ((b2 & 0xc0) === 0x80 && (b3 & 0xc0) === 0x80 &&
              (b4 & 0xc0) === 0x80 &&
              !(b1 === 0xf0 && b2 < 0x90) &&
              !(b1 === 0xf4 && b2 >= 0x90)) {
            i += 3;
            cp = ((b1 & 0x07) << 18) | ((b2 & 0x3f) << 12) |
                ((b3 & 0x3f) << 6) | (b4 & 0x3f);
            appendCodePoint(state, cp);
          } else {
            appendCodePoint(state, utf8Error(decoder));
          }
        } else {
          appendCodePoint(state, utf8Error(decoder));
        }
      }

      state.parts.push(state.chunk);
      return state.parts.join("");
    }

    function TextDecoderPolyfill(label, options) {
      var enc = String(label || "utf-8").toLowerCase();
      if (enc !== "utf-8" && enc !== "utf8" && enc !== "unicode-1-1-utf-8") {
        throw new RangeError("Unsupported TextDecoder encoding");
      }
      options = options || {};
      this.encoding = "utf-8";
      this.fatal = !!options.fatal;
      this.ignoreBOM = !!options.ignoreBOM;
    }

    TextDecoderPolyfill.prototype.decode = function (input) {
      var bytes = bytesFrom(input);
      var text = decodeUtf8(this, bytes);
      if (!this.ignoreBOM && text.charCodeAt(0) === 0xfeff) {
        return text.substring(1);
      }
      return text;
    };

    Object.defineProperty(globalThis, "TextDecoder", {
      value: TextDecoderPolyfill,
      writable: true,
      configurable: true
    });
  }());
}

if (typeof Blob === "undefined") {
  (function () {
    function utf8Bytes(text) {
      var out = [];
      var i;
      var cp;

      text = String(text);
      for (i = 0; i < text.length; i++) {
        cp = text.charCodeAt(i);
        if (cp >= 0xd800 && cp <= 0xdbff && i + 1 < text.length) {
          var next = text.charCodeAt(i + 1);
          if (next >= 0xdc00 && next <= 0xdfff) {
            i++;
            cp = 0x10000 + ((cp - 0xd800) << 10) + (next - 0xdc00);
          }
        }

        if (cp < 0x80) {
          out.push(cp);
        } else if (cp < 0x800) {
          out.push(0xc0 | (cp >> 6), 0x80 | (cp & 0x3f));
        } else if (cp < 0x10000) {
          out.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3f),
              0x80 | (cp & 0x3f));
        } else {
          out.push(0xf0 | (cp >> 18), 0x80 | ((cp >> 12) & 0x3f),
              0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
        }
      }
      return new Uint8Array(out);
    }

    function bytesFromPart(part) {
      if (part instanceof SlateBlob) {
        return part._bytes;
      }
      if (typeof part === "string") {
        return utf8Bytes(part);
      }
      if (ArrayBuffer.isView(part)) {
        return new Uint8Array(part.buffer, part.byteOffset, part.byteLength);
      }
      if (part instanceof ArrayBuffer) {
        return new Uint8Array(part);
      }
      return utf8Bytes(String(part));
    }

    function concatBytes(parts) {
      var arrays = [];
      var size = 0;
      var offset = 0;
      var i;
      var out;

      for (i = 0; i < parts.length; i++) {
        arrays[i] = bytesFromPart(parts[i]);
        size += arrays[i].byteLength;
      }

      out = new Uint8Array(size);
      for (i = 0; i < arrays.length; i++) {
        out.set(arrays[i], offset);
        offset += arrays[i].byteLength;
      }
      return out;
    }

    function copyBytes(bytes) {
      var out = new Uint8Array(bytes.byteLength);
      out.set(bytes);
      return out;
    }

    function SlateBlob(parts, options) {
      options = options || {};
      this._bytes = concatBytes(parts || []);
      this.size = this._bytes.byteLength;
      this.type = options.type ? String(options.type).toLowerCase() : "";
    }

    SlateBlob.prototype.arrayBuffer = function () {
      return Promise.resolve(copyBytes(this._bytes).buffer);
    };

    SlateBlob.prototype.text = function () {
      return Promise.resolve(new TextDecoder("utf-8").decode(this._bytes));
    };

    SlateBlob.prototype.slice = function (start, end, type) {
      var size = this._bytes.byteLength;
      var from = start == null ? 0 : Number(start);
      var to = end == null ? size : Number(end);
      if (from < 0) {
        from = Math.max(size + from, 0);
      } else {
        from = Math.min(from, size);
      }
      if (to < 0) {
        to = Math.max(size + to, 0);
      } else {
        to = Math.min(to, size);
      }
      return new SlateBlob([this._bytes.slice(from, Math.max(from, to))], {
        type: type || ""
      });
    };

    Object.defineProperty(globalThis, "Blob", {
      value: SlateBlob,
      writable: true,
      configurable: true
    });
  }());
}

// DOMTokenList formatter, in theory we can remove this if we do the stringifier IDL support

DOMTokenList.prototype.toString = function () {
  if (this.length == 0) {
    return "";
  }

  var ret = this.item(0);
  for (var index = 1; index < this.length; index++) {
    ret = ret + " " + this.item(index);
  }

  return ret;
}

// Inherit the same toString for settable lists
DOMSettableTokenList.prototype.toString = DOMTokenList.prototype.toString;

(function () {
  function own(obj, name, value) {
    try {
      Object.defineProperty(obj, name, {
        value: value,
        writable: true,
        configurable: true
      });
    } catch (e) {
      obj[name] = value;
    }
  }

  function getter(obj, name, fn) {
    try {
      Object.defineProperty(obj, name, {
        get: fn,
        configurable: true
      });
    } catch (e) {
      /* Some generated bindings may expose non-configurable placeholders. */
    }
  }

  function docFor(node) {
    return node && node.ownerDocument ? node.ownerDocument : document;
  }

  function textOrNode(owner, value) {
    if (value && typeof value === "object" && value.nodeType) {
      return value;
    }
    return owner.createTextNode(String(value));
  }

  function eachNode(args, owner, cb) {
    for (var i = 0; i < args.length; i++) {
      cb(textOrNode(owner, args[i]));
    }
  }

  function elementChildren(node) {
    var out = [];
    var list = node.childNodes || [];
    for (var i = 0; i < list.length; i++) {
      if (list[i] && list[i].nodeType === 1) {
        out.push(list[i]);
      }
    }
    return out;
  }

  function splitClasses(value) {
    return String(value || "").replace(/^\s+|\s+$/g, "").split(/\s+/);
  }

  function hasClass(node, cls) {
    if (!node || node.nodeType !== 1 || !cls) {
      return false;
    }
    if (node.classList && node.classList.contains) {
      return node.classList.contains(cls);
    }
    var classes = splitClasses(node.className);
    for (var i = 0; i < classes.length; i++) {
      if (classes[i] === cls) {
        return true;
      }
    }
    return false;
  }

  function trimSelector(value) {
    return String(value || "").replace(/^\s+|\s+$/g, "");
  }

  function parseSelectorAttr(text) {
    var eq;
    var name;
    var op = "";
    var value = "";

    text = trimSelector(text);
    eq = text.indexOf("=");
    if (eq >= 0) {
      name = trimSelector(text.substring(0, eq));
      value = trimSelector(text.substring(eq + 1));
      if (name.length > 0 && "~|^$*".indexOf(name.charAt(name.length - 1)) >= 0) {
        op = name.charAt(name.length - 1);
        name = trimSelector(name.substring(0, name.length - 1));
      } else {
        op = "=";
      }
      if (value.length >= 2 &&
          ((value.charAt(0) === "\"" && value.charAt(value.length - 1) === "\"") ||
           (value.charAt(0) === "'" && value.charAt(value.length - 1) === "'"))) {
        value = value.substring(1, value.length - 1);
      }
    } else {
      name = text;
    }

    return name ? { name: name, op: op, value: value } : null;
  }

  function parseSimpleSelector(selector) {
    var out = { tag: "", id: "", classes: [], attrs: [] };
    var start = 0;
    var pos = 0;
    var marker;
    var end;
    var attr;

    selector = trimSelector(selector);
    if (!selector) {
      return null;
    }

    while (pos < selector.length &&
        selector.charAt(pos) !== "#" &&
        selector.charAt(pos) !== "." &&
        selector.charAt(pos) !== "[") {
      if (">+~:".indexOf(selector.charAt(pos)) >= 0) {
        return null;
      }
      pos++;
    }
    out.tag = selector.substring(start, pos);

    while (pos < selector.length) {
      marker = selector.charAt(pos++);
      if (marker === "[") {
        end = selector.indexOf("]", pos);
        if (end < 0) {
          return null;
        }
        attr = parseSelectorAttr(selector.substring(pos, end));
        if (!attr) {
          return null;
        }
        out.attrs.push(attr);
        pos = end + 1;
        continue;
      }

      start = pos;
      while (pos < selector.length &&
          selector.charAt(pos) !== "#" &&
          selector.charAt(pos) !== "." &&
          selector.charAt(pos) !== "[") {
        pos++;
      }

      if (marker === "#") {
        out.id = selector.substring(start, pos);
      } else if (marker === ".") {
        out.classes.push(selector.substring(start, pos));
      } else {
        return null;
      }
    }

    return out;
  }

  function attrMatches(node, attr) {
    var value;
    var text;
    var words;
    var i;

    if (!node.getAttribute) {
      return false;
    }
    value = node.getAttribute(attr.name);
    if (value === null || value === undefined) {
      return false;
    }
    if (!attr.op) {
      return true;
    }

    text = String(value);
    if (attr.op === "=") {
      return text === attr.value;
    }
    if (attr.op === "^") {
      return text.indexOf(attr.value) === 0;
    }
    if (attr.op === "$") {
      return text.length >= attr.value.length &&
          text.substring(text.length - attr.value.length) === attr.value;
    }
    if (attr.op === "*") {
      return text.indexOf(attr.value) >= 0;
    }
    if (attr.op === "~") {
      words = splitClasses(text);
      for (i = 0; i < words.length; i++) {
        if (words[i] === attr.value) {
          return true;
        }
      }
      return false;
    }
    if (attr.op === "|") {
      return text === attr.value || text.indexOf(attr.value + "-") === 0;
    }

    return false;
  }

  function simpleMatch(node, selector) {
    var parsed = parseSimpleSelector(selector);
    var i;

    if (!node || node.nodeType !== 1 || !parsed) {
      return false;
    }
    if (selector === "*") {
      return true;
    }
    if (parsed.id && node.id !== parsed.id) {
      return false;
    }
    if (parsed.tag && parsed.tag !== "*" &&
        String(node.tagName || node.nodeName).toLowerCase() !== parsed.tag.toLowerCase()) {
      return false;
    }
    for (i = 0; i < parsed.classes.length; i++) {
      if (!hasClass(node, parsed.classes[i])) {
        return false;
      }
    }
    for (i = 0; i < parsed.attrs.length; i++) {
      if (!attrMatches(node, parsed.attrs[i])) {
        return false;
      }
    }
    return true;
  }

  function selectorMatches(node, selector) {
    var parts = trimSelector(selector).split(/\s+/);
    var idx = parts.length - 1;
    var cur = node;

    if (!parts.length || !parts[0]) {
      return false;
    }
    if (!simpleMatch(cur, parts[idx])) {
      return false;
    }

    for (idx = idx - 1; idx >= 0; idx--) {
      cur = cur.parentElement || cur.parentNode;
      while (cur && !simpleMatch(cur, parts[idx])) {
        cur = cur.parentElement || cur.parentNode;
      }
      if (!cur) {
        return false;
      }
    }

    return true;
  }

  function collect(root, selector) {
    var out = [];
    var selectors = String(selector || "").split(",");
    var all;
    var i;
    var s;

    for (s = 0; s < selectors.length; s++) {
      selectors[s] = selectors[s].replace(/^\s+|\s+$/g, "");
    }

    if (root.getElementsByTagName) {
      all = root.getElementsByTagName("*");
    } else {
      all = [];
    }

    for (i = 0; i < all.length; i++) {
      for (s = 0; s < selectors.length; s++) {
        if (selectorMatches(all[i], selectors[s])) {
          out.push(all[i]);
          break;
        }
      }
    }
    return out;
  }

  function querySelector(selector) {
    var list;
    if (String(selector).charAt(0) === "#" && selector.indexOf(" ") < 0) {
      return document.getElementById(String(selector).substring(1));
    }
    list = collect(this, selector);
    return list.length ? list[0] : null;
  }

  function querySelectorAll(selector) {
    return collect(this, selector);
  }

  function getElementsByClassName(name) {
    return collect(this, "." + name);
  }

  function append() {
    var owner = docFor(this);
    var self = this;
    eachNode(arguments, owner, function (node) {
      self.appendChild(node);
    });
  }

  function prepend() {
    var owner = docFor(this);
    var first = this.firstChild;
    var self = this;
    eachNode(arguments, owner, function (node) {
      self.insertBefore(node, first);
    });
  }

  function before() {
    var parent = this.parentNode;
    var owner = docFor(this);
    var self = this;
    if (!parent) {
      return;
    }
    eachNode(arguments, owner, function (node) {
      parent.insertBefore(node, self);
    });
  }

  function after() {
    var parent = this.parentNode;
    var owner = docFor(this);
    var next = this.nextSibling;
    if (!parent) {
      return;
    }
    eachNode(arguments, owner, function (node) {
      parent.insertBefore(node, next);
    });
  }

  function replaceWith() {
    this.before.apply(this, arguments);
    this.remove();
  }

  function remove() {
    if (this.parentNode) {
      this.parentNode.removeChild(this);
    }
  }

  function closest(selector) {
    var node = this;
    while (node) {
      if (selectorMatches(node, selector)) {
        return node;
      }
      node = node.parentElement || node.parentNode;
    }
    return null;
  }

  if (typeof Node !== "undefined") {
    own(Node.prototype, "append", append);
    own(Node.prototype, "prepend", prepend);
    own(Node.prototype, "before", before);
    own(Node.prototype, "after", after);
    own(Node.prototype, "replaceWith", replaceWith);
    own(Node.prototype, "remove", remove);
  }

  [typeof Document !== "undefined" && Document.prototype,
   typeof Element !== "undefined" && Element.prototype,
   typeof DocumentFragment !== "undefined" && DocumentFragment.prototype].forEach(function (proto) {
    if (!proto) {
      return;
    }
    own(proto, "append", append);
    own(proto, "prepend", prepend);
    own(proto, "querySelector", querySelector);
    own(proto, "querySelectorAll", querySelectorAll);
    own(proto, "getElementsByClassName", getElementsByClassName);
    getter(proto, "children", function () {
      return elementChildren(this);
    });
  });

  if (typeof Element !== "undefined") {
    own(Element.prototype, "matches", function (selector) {
      return selectorMatches(this, selector);
    });
    own(Element.prototype, "closest", closest);
  }

  if (typeof window !== "undefined") {
    if (!window.requestAnimationFrame) {
      window.requestAnimationFrame = function (callback) {
        return setTimeout(function () {
          callback(Date.now());
        }, 16);
      };
    }
    if (!window.cancelAnimationFrame) {
      window.cancelAnimationFrame = function (handle) {
        clearTimeout(handle);
      };
    }
    if (!window.matchMedia) {
      window.matchMedia = function (query) {
        var width = window.innerWidth || (screen && screen.width) || 800;
        var max = /max-width:\s*([0-9]+)px/.exec(query);
        var min = /min-width:\s*([0-9]+)px/.exec(query);
        var matches = true;
        if (max) {
          matches = matches && width <= parseInt(max[1], 10);
        }
        if (min) {
          matches = matches && width >= parseInt(min[1], 10);
        }
        return {
          matches: matches,
          media: String(query),
          addListener: function () {},
          removeListener: function () {},
          addEventListener: function () {},
          removeEventListener: function () {},
          dispatchEvent: function () { return true; }
        };
      };
    }
    if (!window.scrollTo) {
      window.scrollTo = function () {};
    }
    if (!window.CSS) {
      window.CSS = {};
    }
    if (!window.CSS.supports) {
      window.CSS.supports = function () { return false; };
    }
    if (!window.CSS.escape) {
      window.CSS.escape = function (value) {
        return String(value).replace(/[^a-zA-Z0-9_-]/g, "\\$&");
      };
    }
    if (window.devicePixelRatio === undefined) {
      window.devicePixelRatio = 1;
    }
    if (window.innerWidth === undefined) {
      window.innerWidth = screen && screen.width ? screen.width : 800;
    }
    if (window.innerHeight === undefined) {
      window.innerHeight = screen && screen.height ? screen.height : 600;
    }
    if (window.parent === undefined) {
      window.parent = window;
    }
  }

  if (typeof navigator !== "undefined") {
    if (navigator.language === undefined) {
      navigator.language = "en";
    }
    if (navigator.languages === undefined) {
      navigator.languages = ["en"];
    }
    if (navigator.hardwareConcurrency === undefined) {
      navigator.hardwareConcurrency = 1;
    }
    if (navigator.maxTouchPoints === undefined) {
      navigator.maxTouchPoints = 0;
    }
    if (navigator.geolocation === undefined) {
      navigator.geolocation = {
        getCurrentPosition: function () {},
        watchPosition: function () { return 0; },
        clearWatch: function () {}
      };
    }
  }

  var needsURL = false;
  try {
    var testURL = new URL("https://slate.invalid/path?x=1");
    testURL.searchParams.set("y", "2");
    String(testURL);
  } catch (e) {
    needsURL = true;
  }

  if (needsURL) {
    var SlateURL = function URL(value, base) {
      var text = String(value);
      var hashAt;
      var queryAt;
      var originMatch;

      if (base && !/^[a-z][a-z0-9+.-]*:/i.test(text)) {
        if (text.charAt(0) === "/") {
          text = String(base).replace(/^(https?:\/\/[^\/]+).*$/, "$1") + text;
        } else {
          text = String(base).replace(/[^\/]*$/, "") + text;
        }
      }

      hashAt = text.indexOf("#");
      this.hash = hashAt >= 0 ? text.substring(hashAt) : "";
      if (hashAt >= 0) {
        text = text.substring(0, hashAt);
      }

      queryAt = text.indexOf("?");
      this.search = queryAt >= 0 ? text.substring(queryAt) : "";
      this.pathname = queryAt >= 0 ? text.substring(0, queryAt) : text;
      this.protocol = "";
      this.host = "";
      this.hostname = "";
      this.port = "";
      this.origin = "";
      originMatch = /^([a-z][a-z0-9+.-]*:)\/\/([^\/?#]*)(.*)$/i.exec(this.pathname);
      if (originMatch) {
        this.protocol = originMatch[1];
        this.host = originMatch[2];
        this.hostname = this.host.split(":")[0];
        this.port = this.host.indexOf(":") >= 0 ? this.host.split(":").pop() : "";
        this.origin = this.protocol + "//" + this.host;
        this.pathname = originMatch[3] || "/";
      }
      this.href = this.origin + this.pathname + this.search + this.hash;
      this.searchParams = new URLSearchParams(this.search);
    };
    SlateURL.prototype.toString = function () {
      var query = this.searchParams ? this.searchParams.toString() : "";
      return this.origin + this.pathname + (query ? "?" + query : "") + this.hash;
    };
    own(window, "URL", SlateURL);
    URL = SlateURL;
  }

  if (typeof URL !== "undefined") {
    if (!URL.createObjectURL) {
      var blobUrlNextId = 1;
      var blobUrlStore = {};
      URL.createObjectURL = function (blob) {
        var id = "blob:slate/" + blobUrlNextId++;
        blobUrlStore[id] = blob;
        return id;
      };
      URL.revokeObjectURL = function (url) {
        delete blobUrlStore[String(url)];
      };
    } else if (!URL.revokeObjectURL) {
      URL.revokeObjectURL = function () {};
    }
  }

  /* TODO: Replace this with a native binding once generated MessageChannel
   * instances can deliver messages.
   */
  {
    var SlateMessagePort = function MessagePort() {
      this.onmessage = null;
      this._peer = null;
      this._closed = false;
      this._listeners = {};
    };
    SlateMessagePort.prototype.addEventListener = function (type, listener) {
      type = String(type);
      if (!this._listeners[type]) {
        this._listeners[type] = [];
      }
      this._listeners[type].push(listener);
    };
    SlateMessagePort.prototype.removeEventListener = function (type, listener) {
      var list = this._listeners[String(type)] || [];
      var out = [];
      for (var i = 0; i < list.length; i++) {
        if (list[i] !== listener) {
          out.push(list[i]);
        }
      }
      this._listeners[String(type)] = out;
    };
    SlateMessagePort.prototype.dispatchEvent = function (event) {
      var list = this._listeners[String(event && event.type)] || [];
      for (var i = 0; i < list.length; i++) {
        list[i].call(this, event);
      }
      return true;
    };
    SlateMessagePort.prototype.postMessage = function (data) {
      var target = this._peer;
      var event;
      if (!target || target._closed) {
        return;
      }
      event = { type: "message", data: data, target: target, currentTarget: target };
      if (typeof target.onmessage === "function") {
        target.onmessage.call(target, event);
      }
      target.dispatchEvent(event);
    };
    SlateMessagePort.prototype.start = function () {};
    SlateMessagePort.prototype.close = function () {
      this._closed = true;
    };

    var SlateMessageChannel = function MessageChannel() {
      this.port1 = new SlateMessagePort();
      this.port2 = new SlateMessagePort();
      this.port1._peer = this.port2;
      this.port2._peer = this.port1;
    };

    own(window, "MessagePort", SlateMessagePort);
    own(window, "MessageChannel", SlateMessageChannel);
    MessagePort = SlateMessagePort;
    MessageChannel = SlateMessageChannel;
  }

  var needsWorker = false;
  try {
    var testWorker = new Worker("blob:slate/empty-worker");
    if (testWorker && testWorker.terminate) {
      testWorker.terminate();
    }
  } catch (e) {
    needsWorker = true;
  }

  if (needsWorker) {
    var SlateWorker = function Worker(scriptURL) {
      this.scriptURL = String(scriptURL || "");
      this.onmessage = null;
      this.onerror = null;
      this._listeners = {};
    };
    SlateWorker.prototype.addEventListener = function (type, listener) {
      type = String(type);
      if (!this._listeners[type]) {
        this._listeners[type] = [];
      }
      this._listeners[type].push(listener);
    };
    SlateWorker.prototype.removeEventListener = function (type, listener) {
      var list = this._listeners[String(type)] || [];
      var out = [];
      for (var i = 0; i < list.length; i++) {
        if (list[i] !== listener) {
          out.push(list[i]);
        }
      }
      this._listeners[String(type)] = out;
    };
    SlateWorker.prototype.dispatchEvent = function (event) {
      var list = this._listeners[String(event && event.type)] || [];
      for (var i = 0; i < list.length; i++) {
        list[i].call(this, event);
      }
      return true;
    };
    SlateWorker.prototype.postMessage = function () {};
    SlateWorker.prototype.terminate = function () {};
    own(window, "Worker", SlateWorker);
    Worker = SlateWorker;
  }

  var needsURLSearchParams = false;
  try {
    if (typeof URLSearchParams === "undefined") {
      needsURLSearchParams = true;
    } else {
      new URLSearchParams("slate=1");
    }
  } catch (e) {
    needsURLSearchParams = true;
  }

  if (needsURLSearchParams) {
    var SlateURLSearchParams = function (init) {
      this._pairs = [];
      if (typeof init === "string") {
        this._parse(init);
      } else if (init && typeof init === "object") {
        for (var key in init) {
          if (Object.prototype.hasOwnProperty.call(init, key)) {
            this.append(key, init[key]);
          }
        }
      }
    };
    SlateURLSearchParams.prototype._parse = function (text) {
      var source = String(text || "").replace(/^\?/, "");
      var parts = source ? source.split("&") : [];
      for (var i = 0; i < parts.length; i++) {
        var eq = parts[i].indexOf("=");
        if (eq >= 0) {
          this.append(decodeURIComponent(parts[i].substring(0, eq)), decodeURIComponent(parts[i].substring(eq + 1)));
        } else if (parts[i]) {
          this.append(decodeURIComponent(parts[i]), "");
        }
      }
    };
    SlateURLSearchParams.prototype.append = function (key, value) {
      this._pairs.push([String(key), String(value)]);
    };
    SlateURLSearchParams.prototype.set = function (key, value) {
      this.delete(key);
      this.append(key, value);
    };
    SlateURLSearchParams.prototype.get = function (key) {
      key = String(key);
      for (var i = 0; i < this._pairs.length; i++) {
        if (this._pairs[i][0] === key) {
          return this._pairs[i][1];
        }
      }
      return null;
    };
    SlateURLSearchParams.prototype.has = function (key) {
      return this.get(key) !== null;
    };
    SlateURLSearchParams.prototype.delete = function (key) {
      var out = [];
      key = String(key);
      for (var i = 0; i < this._pairs.length; i++) {
        if (this._pairs[i][0] !== key) {
          out.push(this._pairs[i]);
        }
      }
      this._pairs = out;
    };
    SlateURLSearchParams.prototype.toString = function () {
      var out = [];
      for (var i = 0; i < this._pairs.length; i++) {
        out.push(encodeURIComponent(this._pairs[i][0]) + "=" + encodeURIComponent(this._pairs[i][1]));
      }
      return out.join("&");
    };
    if (typeof window !== "undefined") {
      own(window, "URLSearchParams", SlateURLSearchParams);
    }
    URLSearchParams = SlateURLSearchParams;
  }

  if (typeof window.FormData === "undefined") {
    var formControlValue = function (control, fallback) {
      var value = control.value;
      if (value === undefined || value === null) {
        value = control.getAttribute && control.getAttribute("value");
      }
      if (value === undefined || value === null) {
        value = fallback || "";
      }
      return String(value);
    };
    var optionValue = function (option) {
      var value = option.getAttribute && option.getAttribute("value");
      return value === null || value === undefined ?
        String(option.textContent || "") : String(value);
    };
    var SlateFormData = function FormData(form) {
      this._pairs = [];
      if (form && typeof form.getElementsByTagName === "function") {
        this._readForm(form);
      }
    };
    SlateFormData.prototype._appendControl = function (control) {
      if (!control || !control.getAttribute) {
        return;
      }

      var name = control.getAttribute("name");
      if (!name || control.hasAttribute("disabled")) {
        return;
      }

      var tag = String(control.nodeName || "").toLowerCase();
      var type = String(control.getAttribute("type") || "text").toLowerCase();
      if (tag === "button" || type === "button" || type === "reset" ||
          type === "submit" || type === "file") {
        return;
      }

      if (type === "checkbox" || type === "radio") {
        if (!(control.checked || control.hasAttribute("checked"))) {
          return;
        }
        this.append(name, formControlValue(control, "on"));
        return;
      }

      if (tag === "select") {
        var options = control.getElementsByTagName("option");
        var selected = false;
        for (var i = 0; i < options.length; i++) {
          if (options[i].selected || options[i].hasAttribute("selected")) {
            this.append(name, optionValue(options[i]));
            selected = true;
          }
        }
        if (!selected && options.length > 0 && !control.hasAttribute("multiple")) {
          this.append(name, optionValue(options[0]));
        }
        return;
      }

      this.append(name, formControlValue(control, ""));
    };
    SlateFormData.prototype._readForm = function (form) {
      var tags = ["input", "select", "textarea", "button"];
      for (var t = 0; t < tags.length; t++) {
        var controls = form.getElementsByTagName(tags[t]);
        for (var i = 0; i < controls.length; i++) {
          this._appendControl(controls[i]);
        }
      }
    };
    SlateFormData.prototype.append = function (name, value) {
      this._pairs.push([String(name), value == null ? "" : String(value)]);
    };
    SlateFormData.prototype.delete = function (name) {
      var key = String(name);
      var out = [];
      for (var i = 0; i < this._pairs.length; i++) {
        if (this._pairs[i][0] !== key) {
          out.push(this._pairs[i]);
        }
      }
      this._pairs = out;
    };
    SlateFormData.prototype.get = function (name) {
      var key = String(name);
      for (var i = 0; i < this._pairs.length; i++) {
        if (this._pairs[i][0] === key) {
          return this._pairs[i][1];
        }
      }
      return null;
    };
    SlateFormData.prototype.getAll = function (name) {
      var key = String(name);
      var out = [];
      for (var i = 0; i < this._pairs.length; i++) {
        if (this._pairs[i][0] === key) {
          out.push(this._pairs[i][1]);
        }
      }
      return out;
    };
    SlateFormData.prototype.has = function (name) {
      return this.get(name) !== null;
    };
    SlateFormData.prototype.set = function (name, value) {
      this.delete(name);
      this.append(name, value);
    };
    SlateFormData.prototype.entries = function () {
      return this._pairs.slice()[Symbol.iterator]();
    };
    SlateFormData.prototype.keys = function () {
      var keys = [];
      for (var i = 0; i < this._pairs.length; i++) {
        keys.push(this._pairs[i][0]);
      }
      return keys[Symbol.iterator]();
    };
    SlateFormData.prototype.values = function () {
      var values = [];
      for (var i = 0; i < this._pairs.length; i++) {
        values.push(this._pairs[i][1]);
      }
      return values[Symbol.iterator]();
    };
    SlateFormData.prototype.forEach = function (callback, thisArg) {
      for (var i = 0; i < this._pairs.length; i++) {
        callback.call(thisArg, this._pairs[i][1], this._pairs[i][0], this);
      }
    };
    SlateFormData.prototype[Symbol.iterator] =
      SlateFormData.prototype.entries;
    own(window, "FormData", SlateFormData);
    FormData = SlateFormData;
  }

  if (typeof window.Turbo === "undefined") {
    own(window, "Turbo", {
      session: {
        drive: true
      },
      visit: function (url) {
        window.location.href = String(url);
      },
      clearCache: function () {}
    });
    Turbo = window.Turbo;
  } else if (!window.Turbo.session) {
    window.Turbo.session = {
      drive: true
    };
  }

  if (typeof window.Headers === "undefined") {
    var SlateHeaders = function Headers(init) {
      this._values = {};
      if (init) {
        for (var key in init) {
          if (Object.prototype.hasOwnProperty.call(init, key)) {
            this.set(key, init[key]);
          }
        }
      }
    };
    SlateHeaders.prototype.get = function (key) {
      key = String(key).toLowerCase();
      return Object.prototype.hasOwnProperty.call(this._values, key) ?
        this._values[key] : null;
    };
    SlateHeaders.prototype.set = function (key, value) {
      this._values[String(key).toLowerCase()] = String(value);
    };
    own(window, "Headers", SlateHeaders);
    Headers = SlateHeaders;
  }

  if (typeof window.Response === "undefined") {
    var SlateResponse = function Response(body, options) {
      options = options || {};
      this._body = body == null ? "" : String(body);
      this.status = options.status || 200;
      this.statusText = options.statusText || "";
      this.ok = this.status >= 200 && this.status < 300;
      this.headers = new Headers(options.headers || {});
    };
    SlateResponse.prototype.text = function () {
      return Promise.resolve(this._body);
    };
    SlateResponse.prototype.json = function () {
      return this.text().then(function (text) {
        return JSON.parse(text);
      });
    };
    SlateResponse.prototype.blob = function () {
      return Promise.resolve(new Blob([this._body], {
        type: this.headers.get("content-type") || "text/plain"
      }));
    };
    own(window, "Response", SlateResponse);
    Response = SlateResponse;
  }

  if (typeof window.fetch === "undefined") {
    /* TODO: Replace this with a native fetch binding backed by llcache. */
    own(window, "fetch", function fetch() {
      return Promise.resolve(new Response("", { status: 204 }));
    });
    fetch = window.fetch;
  }
}());
