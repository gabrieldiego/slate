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

  function simpleMatch(node, selector) {
    var tag = "";
    var cls = "";
    var id = "";
    var hash;
    var dot;

    selector = String(selector || "").replace(/^\s+|\s+$/g, "");
    if (!node || node.nodeType !== 1 || selector === "") {
      return false;
    }
    if (selector === "*") {
      return true;
    }

    hash = selector.indexOf("#");
    if (hash >= 0) {
      id = selector.substring(hash + 1);
      selector = selector.substring(0, hash);
      dot = id.indexOf(".");
      if (dot >= 0) {
        cls = id.substring(dot + 1);
        id = id.substring(0, dot);
      }
    }

    dot = selector.indexOf(".");
    if (dot >= 0) {
      tag = selector.substring(0, dot);
      cls = selector.substring(dot + 1);
    } else if (selector.charAt(0) === ".") {
      cls = selector.substring(1);
    } else if (selector.charAt(0) === "#") {
      id = selector.substring(1);
    } else {
      tag = selector;
    }

    if (id && node.id !== id) {
      return false;
    }
    if (tag && tag !== "*" && String(node.tagName || node.nodeName).toLowerCase() !== tag.toLowerCase()) {
      return false;
    }
    if (cls && !hasClass(node, cls)) {
      return false;
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
        if (simpleMatch(all[i], selectors[s])) {
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
      if (simpleMatch(node, selector)) {
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
      return simpleMatch(this, selector);
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
}());
