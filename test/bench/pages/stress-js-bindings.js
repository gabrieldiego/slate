(function () {
	var status = document.getElementById("binding-status");
	var log = document.getElementById("binding-log");
	var board = document.getElementById("binding-board");
	var touched = 0;
	var failures = 0;

	function setText(node, text) {
		node.innerHTML = "";
		node.appendChild(document.createTextNode(text));
	}

	function addLog(text) {
		var item = document.createElement("li");
		item.appendChild(document.createTextNode(text));
		log.appendChild(item);
	}

	function safe(label, fn) {
		try {
			fn();
			touched++;
			return true;
		} catch (err) {
			failures++;
			addLog(label + " skipped: " + (err && err.name ? err.name : "error"));
			return false;
		}
	}

	function firstByTag(name) {
		var nodes = document.getElementsByTagName(name);
		if (nodes && nodes.length > 0 && nodes.item) {
			return nodes.item(0);
		}
		return null;
	}

	function touchNode(node, label) {
		if (!node) {
			return;
		}
		safe(label + " node basics", function () {
			var text = node.nodeName + ":" + node.nodeType;
			text += ":" + (node.ownerDocument ? "doc" : "nodoc");
			text += ":" + (node.parentNode ? "parent" : "root");
			if (node.childNodes) {
				text += ":" + node.childNodes.length;
			}
			node.normalize();
			addLog(text);
		});
		safe(label + " element basics", function () {
			if (node.setAttribute) {
				node.setAttribute("data-probed", label);
				node.getAttribute("data-probed");
				node.hasAttribute("data-probed");
				node.attributes;
				node.removeAttribute("data-probed");
			}
			if (node.className !== undefined) {
				node.className = (node.className ? node.className + " " : "") + "probed";
			}
			if (node.style) {
				node.style.borderColor = "#607080";
				node.style.setProperty("outline", "1px solid #d0d7de", "");
				node.style.getPropertyValue("outline");
				node.style.removeProperty("outline");
			}
			if (node.dataset) {
				node.dataset.bindingProbe = label;
			}
			if (node.classList) {
				node.classList.add("classlist-probed");
				node.classList.contains("classlist-probed");
				node.classList.remove("classlist-probed");
			}
		});
		safe(label + " tree operations", function () {
			var clone = node.cloneNode(false);
			var holder = document.createElement("div");
			holder.appendChild(clone);
			holder.insertBefore(document.createTextNode("prefix"), clone);
			holder.removeChild(clone);
			board.appendChild(holder);
			board.removeChild(holder);
		});
		safe(label + " geometry properties", function () {
			var total = 0;
			total += node.clientWidth || 0;
			total += node.clientHeight || 0;
			total += node.offsetWidth || 0;
			total += node.offsetHeight || 0;
			total += node.scrollWidth || 0;
			total += node.scrollHeight || 0;
			node.scrollTop = node.scrollTop || 0;
			node.scrollLeft = node.scrollLeft || 0;
			addLog(label + " geometry " + total);
		});
		safe(label + " selectors", function () {
			if (node.matches) {
				node.matches("*");
			}
			if (node.querySelector) {
				node.querySelector("*");
			}
			if (node.querySelectorAll) {
				node.querySelectorAll("*");
			}
			if (node.insertAdjacentHTML) {
				node.insertAdjacentHTML("beforeend", "<span>adjacent</span>");
			}
		});
	}

	function touchNamedElements() {
		var tags = [
			"html", "head", "base", "meta", "link", "style", "title", "body",
			"section", "article", "span", "data", "time", "ins", "del", "pre", "hr",
			"details", "dialog", "menu", "menuitem", "dir", "dl", "ol", "ul",
			"font", "marquee", "applet", "keygen", "form", "fieldset", "legend", "input", "select",
			"optgroup", "option", "textarea", "button", "datalist", "output",
			"meter", "progress", "table", "caption", "col", "thead", "tbody",
			"tfoot", "tr", "th", "td", "picture", "source", "img", "map",
			"area", "object", "param", "embed", "iframe", "audio", "video",
			"track", "canvas", "template", "slate-unknown", "script"
		];
		tags.forEach(function (tag) {
			safe("tag " + tag, function () {
				touchNode(firstByTag(tag), tag);
			});
		});
	}

	function touchForms() {
		var form = document.getElementById("binding-form");
		var input = document.getElementById("binding-input");
		var checkbox = document.getElementById("binding-check");
		var select = document.getElementById("binding-select");
		var textarea = document.getElementById("binding-textarea");
		var output = document.getElementById("binding-output");
		var progress = document.getElementById("binding-progress");
		var meter = document.getElementById("binding-meter");

		safe("form properties", function () {
			form.name = "binding-form-name";
			form.method = "post";
			form.action = "about:blank";
			form.target = "_self";
			form.noValidate = true;
			form.acceptCharset = "utf-8";
			form.elements;
			form.length;
			form.checkValidity();
			form.reportValidity();
			form.requestAutocomplete();
		});
		safe("input properties", function () {
			input.value = input.value + " updated";
			input.defaultValue = "default binding";
			input.disabled = false;
			input.readOnly = false;
			input.size = 28;
			input.maxLength = 80;
			input.name = "text-updated";
			input.type;
			input.form;
			input.labels;
			input.checkValidity();
			input.reportValidity();
			input.setCustomValidity("");
			checkbox.checked = !checkbox.checked;
		});
		safe("select properties", function () {
			select.selectedIndex = 1;
			select.value = "two";
			select.name = "select-updated";
			select.size = 1;
			select.multiple = false;
			select.required = false;
			select.options;
			select.length;
			select.selectedOptions;
			select.item(0);
			select.namedItem("missing");
			select.checkValidity();
			select.reportValidity();
			select.setCustomValidity("");
		});
		safe("textarea properties", function () {
			textarea.value = textarea.value + " plus typed stress coverage";
			textarea.defaultValue = "default textarea";
			textarea.rows = 6;
			textarea.cols = 38;
			textarea.name = "notes-updated";
			textarea.wrap = "soft";
			textarea.checkValidity();
			textarea.reportValidity();
			textarea.setCustomValidity("");
		});
		safe("meter progress output", function () {
			progress.value = 82;
			progress.max = 120;
			meter.value = 7;
			meter.min = 0;
			meter.max = 10;
			output.value = "output updated";
			setText(output, output.value);
		});
		setText(status, "Stress JS bindings form probes complete");
		console.log("stress-js-bindings-forms");
	}

	function touchCanvas() {
		var canvas = document.getElementById("binding-canvas");
		safe("canvas properties", function () {
			canvas.width = 220;
			canvas.height = 120;
			canvas.probablySupportsContext("2d");
			canvas.setContext({});
			canvas.toDataURL();
			canvas.toBlob(function () {});
			canvas.transferControlToProxy();
		});
		safe("canvas 2d drawing", function () {
			var ctx = canvas.getContext("2d");
			var gradient;
			if (!ctx) {
				return;
			}
			ctx.save();
			ctx.globalAlpha = 0.95;
			ctx.fillStyle = "#f8fafc";
			ctx.strokeStyle = "#304050";
			ctx.lineWidth = 2;
			ctx.lineCap = "round";
			ctx.lineJoin = "round";
			ctx.miterLimit = 4;
			ctx.shadowOffsetX = 1;
			ctx.shadowOffsetY = 1;
			ctx.shadowBlur = 0;
			ctx.shadowColor = "#d0d7de";
			ctx.font = "12px sans-serif";
			ctx.textAlign = "left";
			ctx.textBaseline = "top";
			ctx.direction = "ltr";
			ctx.fillRect(0, 0, 220, 120);
			ctx.clearRect(6, 6, 24, 16);
			ctx.strokeRect(8, 8, 204, 104);
			ctx.beginPath();
			ctx.moveTo(18, 82);
			ctx.lineTo(62, 44);
			ctx.quadraticCurveTo(90, 20, 122, 52);
			ctx.bezierCurveTo(136, 68, 158, 78, 190, 28);
			ctx.arc(54, 66, 12, 0, Math.PI);
			ctx.rect(144, 70, 34, 24);
			ctx.closePath();
			ctx.stroke();
			ctx.fillText("canvas stress", 20, 20);
			ctx.strokeText("outline", 20, 36);
			ctx.measureText("canvas stress");
			gradient = ctx.createLinearGradient(0, 0, 220, 0);
			if (gradient && gradient.addColorStop) {
				gradient.addColorStop(0, "#2f6f9f");
				gradient.addColorStop(1, "#9f5f2f");
				ctx.fillStyle = gradient;
			}
			ctx.fillRect(20, 92, 160, 12);
			ctx.setLineDash([4, 2]);
			ctx.getLineDash();
			ctx.lineDashOffset = 1;
			ctx.resetTransform();
			ctx.translate(2, 2);
			ctx.scale(1, 1);
			ctx.rotate(0);
			ctx.transform(1, 0, 0, 1, 0, 0);
			ctx.setTransform(1, 0, 0, 1, 0, 0);
			ctx.clip();
			ctx.resetClip();
			ctx.isPointInPath(10, 10);
			ctx.isPointInStroke(10, 10);
			ctx.createImageData(4, 4);
			ctx.getImageData(0, 0, 1, 1);
			ctx.restore();
			ctx.commit();
		});
	}

	function touchEvents() {
		var button = document.getElementById("binding-button");
		var seen = [];
		function listener(evt) {
			seen.push(evt.type);
			safe("event properties", function () {
				evt.type;
				evt.target;
				evt.currentTarget;
				evt.eventPhase;
				evt.bubbles;
				evt.cancelable;
				evt.defaultPrevented;
				evt.timeStamp;
				evt.isTrusted;
				evt.preventDefault();
				evt.stopPropagation();
			});
		}
		function touchProps(evt, names) {
			names.forEach(function (name) {
				evt[name];
			});
		}
		function typedEvent(label, eventInterface, eventType, initMethod, initArgs, props) {
			safe(label, function () {
				var evt = document.createEvent(eventInterface);
				button.addEventListener(eventType, listener, false);
				if (initMethod && evt[initMethod]) {
					evt[initMethod].apply(evt, initArgs);
				}
				if (evt.initEvent) {
					evt.initEvent(eventType, true, true);
				}
				touchProps(evt, props || []);
				button.dispatchEvent(evt);
				button.removeEventListener(eventType, listener, false);
			});
		}
		safe("event target", function () {
			button.addEventListener("binding-event", listener, false);
			var evt = document.createEvent("Event");
			evt.initEvent("binding-event", true, true);
			button.dispatchEvent(evt);
			button.removeEventListener("binding-event", listener, false);
		});
		safe("mouse event init", function () {
			var evt = document.createEvent("MouseEvent");
			if (evt.initMouseEvent) {
				evt.initMouseEvent("click", true, true, window, 1, 10, 11, 12, 13, false, false, false, false, 0, null);
				evt.initEvent("click", true, true);
				button.dispatchEvent(evt);
				evt.clientX;
				evt.clientY;
				evt.screenX;
				evt.screenY;
				evt.ctrlKey;
				evt.shiftKey;
				evt.altKey;
				evt.metaKey;
				evt.button;
				evt.buttons;
				evt.getModifierState("Shift");
			}
		});
		safe("keyboard event init", function () {
			var evt = document.createEvent("KeyboardEvent");
			if (evt.initKeyboardEvent) {
				evt.initKeyboardEvent("keydown", true, true, window, "A", 0, "", false, "");
				evt.initEvent("keydown", true, true);
				button.dispatchEvent(evt);
				evt.key;
				evt.code;
				evt.location;
				evt.ctrlKey;
				evt.shiftKey;
				evt.altKey;
				evt.metaKey;
				evt.repeat;
				evt.isComposing;
				evt.charCode;
				evt.keyCode;
				evt.which;
				evt.getModifierState("Control");
			}
		});
		safe("custom event init", function () {
			var evt = document.createEvent("CustomEvent");
			if (evt.initCustomEvent) {
				evt.initCustomEvent("bench-custom", true, true, "detail");
				button.dispatchEvent(evt);
				evt.detail;
			}
		});
		typedEvent("ui event init", "UIEvent", "load", "initUIEvent",
			["load", true, true, window, 4],
			["view", "detail"]);
		typedEvent("focus event init", "FocusEvent", "focus", "initFocusEvent",
			["focus", false, false, window, 1, button],
			["relatedTarget"]);
		typedEvent("wheel event init", "WheelEvent", "wheel", "initWheelEvent",
			["wheel", true, true, window, 1, 20, 21, 22, 23, 0, null, "", 1, -2, 0, 0],
			["deltaX", "deltaY", "deltaZ", "deltaMode", "DOM_DELTA_PIXEL", "DOM_DELTA_LINE", "DOM_DELTA_PAGE"]);
		typedEvent("drag event init", "DragEvent", "dragstart", null, [],
			["dataTransfer", "clientX", "clientY", "button"]);
		typedEvent("composition event init", "CompositionEvent", "compositionstart", "initCompositionEvent",
			["compositionstart", true, true, window, "data", ""],
			["data"]);
		typedEvent("mutation event init", "MutationEvent", "DOMAttrModified", "initMutationEvent",
			["DOMAttrModified", true, true, button, "old", "new", "data-probed", 1],
			["relatedNode", "prevValue", "newValue", "attrName", "attrChange"]);
		typedEvent("message event init", "MessageEvent", "message", "initMessageEvent",
			["message", false, false, "payload", "file://bench", "last", window, []],
			["data", "origin", "lastEventId", "source", "ports"]);
		typedEvent("storage event getters", "StorageEvent", "storage", null, [],
			["key", "oldValue", "newValue", "url", "storageArea"]);
		typedEvent("close event getters", "CloseEvent", "close", null, [],
			["wasClean", "code", "reason"]);
		typedEvent("error event getters", "ErrorEvent", "error", null, [],
			["message", "filename", "lineno", "colno", "error"]);
		typedEvent("before unload event getters", "BeforeUnloadEvent", "beforeunload", null, [],
			["returnValue"]);
		typedEvent("page transition getters", "PageTransitionEvent", "pageshow", null, [],
			["persisted"]);
		typedEvent("hash change getters", "HashChangeEvent", "hashchange", null, [],
			["oldURL", "newURL"]);
		typedEvent("pop state getters", "PopStateEvent", "popstate", null, [],
			["state"]);
		typedEvent("related event getters", "RelatedEvent", "related", null, [],
			["relatedTarget"]);
		typedEvent("autocomplete error getters", "AutocompleteErrorEvent", "autocompleteerror", null, [],
			["reason"]);
		typedEvent("track event getters", "TrackEvent", "track", null, [],
			["track"]);
		setText(status, "Stress JS bindings events complete " + seen.length);
		console.log("stress-js-bindings-events");
	}

	function touchDocumentAndWindow() {
		safe("document collections", function () {
			document.documentElement;
			document.head;
			document.body;
			document.images;
			document.links;
			document.forms;
			document.scripts;
			document.styleSheets;
			document.getElementsByClassName("bindings-card");
			document.getElementsByTagNameNS("*", "*");
			document.getElementsByName("text");
			document.querySelector(".bindings-card");
			document.querySelectorAll(".bindings-card");
			document.createNodeIterator(document.body, -1, null);
			document.createTreeWalker(document.body, -1, null);
		});
		safe("window navigator history location", function () {
			var text = "";
			text += navigator.appCodeName;
			text += navigator.appName;
			text += navigator.appVersion;
			text += navigator.platform;
			text += navigator.product;
			text += navigator.productSub;
			text += navigator.userAgent;
			text += navigator.vendor;
			text += navigator.vendorSub;
			text += navigator.language;
			text += navigator.languages;
			text += navigator.onLine;
			text += navigator.cookieEnabled;
			text += navigator.plugins;
			text += navigator.mimeTypes;
			navigator.javaEnabled();
			history.length;
			history.state;
			location.href;
			location.origin;
			location.protocol;
			location.host;
			location.hostname;
			location.port;
			location.pathname;
			location.search;
			location.hash;
			window.innerWidth;
			window.innerHeight;
			window.pageXOffset;
			window.pageYOffset;
			window.screen;
			window.applicationCache;
			window.localStorage;
			window.sessionStorage;
			addLog("window probes " + text.length);
		});
		safe("storage operations", function () {
			localStorage.setItem("binding-key", "binding-value");
			localStorage.getItem("binding-key");
			localStorage.key(0);
			localStorage.length;
			localStorage.removeItem("binding-key");
			sessionStorage.setItem("binding-key", "binding-value");
			sessionStorage.getItem("binding-key");
			sessionStorage.clear();
		});
		safe("parser and range probes", function () {
			var range = document.createRange();
			if (range) {
				range.selectNodeContents(document.getElementById("binding-elements"));
				range.cloneContents();
				range.cloneRange();
				range.collapse(false);
				range.detach();
			}
			if (window.DOMParser) {
				var parser = new DOMParser();
				if (parser.parseFromString) {
					parser.parseFromString("<p>parsed</p>", "text/html");
				}
			}
		});
	}

	function bindDriverEvents() {
		var mouseSeen = false;
		var keySeen = false;
		var spans = document.getElementsByTagName("span");
		var input = document.getElementById("binding-input");
		var textarea = document.getElementById("binding-textarea");
		var span;
		function driverMouse(evt) {
			if (mouseSeen) {
				return;
			}
			mouseSeen = true;
			safe("driver mouse event", function () {
				evt.type;
				evt.target;
				evt.clientX;
				evt.clientY;
				evt.screenX;
				evt.screenY;
				evt.button;
				evt.buttons;
				evt.getModifierState("Shift");
			});
			console.log("stress-js-bindings-driver-mouse");
		}
		function driverKey(evt) {
			if (keySeen) {
				return;
			}
			keySeen = true;
			safe("driver keyboard event", function () {
				evt.type;
				evt.target;
				evt.key;
				evt.code;
				evt.location;
				evt.ctrlKey;
				evt.shiftKey;
				evt.altKey;
				evt.metaKey;
				evt.getModifierState("Control");
			});
			console.log("stress-js-bindings-driver-key");
		}
		for (var i = 0; spans && spans.item && (span = spans.item(i)); i++) {
			if (span.className && span.className.indexOf("cursor-") >= 0) {
				span.addEventListener("mousemove", driverMouse, false);
				span.addEventListener("mouseover", driverMouse, false);
			}
		}
		if (input) {
			input.addEventListener("keydown", driverKey, false);
		}
		if (textarea) {
			textarea.addEventListener("keypress", driverKey, false);
		}
	}

	document.getElementById("binding-run-events").onclick = function () {
		touchEvents();
	};
	document.getElementById("binding-run-forms").onclick = function () {
		touchForms();
	};
	document.getElementById("binding-form").onsubmit = function (evt) {
		if (evt && evt.preventDefault) {
			evt.preventDefault();
		}
		touchForms();
		return false;
	};
	document.getElementById("binding-button").onclick = function (evt) {
		safe("real click event", function () {
			evt.type;
			evt.target;
			evt.currentTarget;
			evt.bubbles;
			evt.cancelable;
			evt.defaultPrevented;
			evt.preventDefault();
		});
		setText(status, "Stress JS bindings real click complete");
		console.log("stress-js-bindings-click");
	};

	bindDriverEvents();
	touchNamedElements();
	touchDocumentAndWindow();
	touchCanvas();
	setText(status, "Stress JS bindings ready, probes " + touched + ", skipped " + failures);
	console.log("stress-js-bindings-ready");
}());
