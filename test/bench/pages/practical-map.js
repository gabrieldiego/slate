(function () {
	var map = document.querySelector("#map");
	var mapUi = document.querySelector("#map-ui");
	var pane = document.querySelector("#leaflet-map-pane");
	var grid = document.querySelector("#tile-grid");
	var status = document.querySelector("#map-status");
	var popup = document.querySelector("#map-popup");
	var zoomText = document.querySelector("#map-zoom");
	var centerText = document.querySelector("#map-center");
	var nearby = document.querySelector("#map-nearby");
	var contextMenu = document.querySelector("#map-context-menu");
	var mapToolsButton = document.querySelector("#map-tools-button");
	var mapToolsMenu = document.querySelector("#map-tools-menu");
	var languageDialog = document.querySelector("#map-language-dialog");
	var routeFrom = document.querySelector("#route-from");
	var routeTo = document.querySelector("#route-to");
	var mapLoader = document.querySelector("#map-loader");
	var controls = [];
	var svgCount = document.getElementsByTagName("svg").length;
	var tileCount = 0;
	var offsetX = 0;
	var offsetY = 0;
	var zoom = parseInt(map.getAttribute("data-zoom"), 10) || 12;
	var dragging = false;
	var dragStartX = 0;
	var dragStartY = 0;
	var dragOriginX = 0;
	var dragOriginY = 0;
	var i;

	function setText(node, text) {
		node.innerHTML = "";
		node.appendChild(document.createTextNode(text));
	}

	function markerSvg(color, text) {
		return "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='128' height='128' viewBox='0 0 128 128'%3E" +
			"%3Crect width='128' height='128' fill='" + encodeURIComponent(color) + "'/%3E" +
			"%3Cpath d='M0 72 C22 42 46 88 72 56 C94 30 104 44 128 26' fill='none' stroke='white' stroke-width='10' opacity='.7'/%3E" +
			"%3Cpath d='M10 96 L118 22' stroke='%238a7a5c' stroke-width='4' opacity='.6'/%3E" +
			"%3Ctext x='12' y='118' font-size='18' font-family='Arial' fill='%23304050'%3E" + encodeURIComponent(text) + "%3C/text%3E" +
			"%3C/svg%3E";
	}

	function updateTransform() {
		requestAnimationFrame(function () {
			pane.style.left = offsetX + "px";
			pane.style.top = offsetY + "px";
			pane.style.transform = "translate3d(" + offsetX + "px," + offsetY + "px,0)";
			map.setAttribute("data-zoom", String(zoom));
			setText(centerText, offsetX + "," + offsetY);
			setText(zoomText, zoom);
			setText(status, "Map center " + offsetX + "," + offsetY + " zoom " + zoom);
			console.log("map-frame-" + offsetX + "," + offsetY);
		});
	}

	function zoomBy(delta, source) {
		var nextZoom = zoom + delta;
		if (nextZoom < 1) {
			nextZoom = 1;
		} else if (nextZoom > 19) {
			nextZoom = 19;
		}
		if (nextZoom === zoom) {
			console.log(source + "-unchanged");
			return;
		}
		zoom = nextZoom;
		updateTransform();
		console.log(source);
	}

	function addTile(x, y, kind) {
		var tile = document.createElement("img");
		var label = "z" + zoom + " / " + x + "," + y;
		var colors = {
			water: "#9dc8dd",
			park: "#acd6a4",
			city: "#dfd3bd"
		};

		tile.className = "map-tile leaflet-tile";
		tile.setAttribute("alt", label);
		tile.setAttribute("data-tile-kind", kind);
		tile.setAttribute("data-x", x);
		tile.setAttribute("data-y", y);
		tile.style.left = (x * 128) + "px";
		tile.style.top = (y * 128) + "px";
		tile.addEventListener("load", function () {
			this.classList.add("leaflet-tile-loaded");
		}, false);
		tile.src = markerSvg(colors[kind] || colors.city, label);
		tile.classList.add("leaflet-tile-loaded");
		grid.append(tile);
		tileCount++;
	}

	function addControlIcon() {
		var icon = document.createElementNS("http://www.w3.org/2000/svg", "svg");
		var circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
		icon.setAttributeNS(null, "width", "18");
		icon.setAttributeNS(null, "height", "18");
		icon.setAttributeNS(null, "viewBox", "0 0 18 18");
		icon.classList.add("control-icon");
		circle.setAttributeNS(null, "cx", "9");
		circle.setAttributeNS(null, "cy", "9");
		circle.setAttributeNS(null, "r", "6");
		circle.setAttributeNS(null, "fill", "none");
		circle.setAttributeNS(null, "stroke", "currentColor");
		circle.setAttributeNS(null, "stroke-width", "2");
		icon.append(circle);
		document.getElementById("map-search-button").append(icon);
	}

	function toggleDropdown(button, menu) {
		var expanded = button.getAttribute("aria-expanded") === "true";
		if (expanded) {
			menu.classList.remove("show");
			button.setAttribute("aria-expanded", "false");
			console.log("map-dropdown-closed");
		} else {
			menu.classList.add("show");
			button.setAttribute("aria-expanded", "true");
			console.log("map-dropdown-open");
		}
	}

	function openModal(dialog) {
		dialog.classList.add("map-modal-open");
		dialog.style.visibility = "visible";
		dialog.setAttribute("aria-hidden", "false");
		console.log("map-modal-open");
	}

	function closeModal(dialog) {
		dialog.classList.remove("map-modal-open");
		dialog.style.visibility = "hidden";
		dialog.setAttribute("aria-hidden", "true");
		console.log("map-modal-close");
	}

	function countElementsWithAttribute(tagName, attributeName, value) {
		var nodes = document.getElementsByTagName(tagName);
		var count = 0;
		var current;
		var i;

		for (i = 0; i < nodes.length; i++) {
			current = nodes[i].getAttribute(attributeName);
			if (value === undefined ? current !== null : current === value) {
				count++;
			}
		}

		return count;
	}

	function findTargetBySelectorId(selector) {
		if (selector && selector.charAt(0) === "#") {
			return document.getElementById(selector.substring(1));
		}
		return null;
	}

	function bindBootstrapLikeControls() {
		var buttons = document.getElementsByTagName("button");
		var i;

		for (i = 0; i < buttons.length; i++) {
			(function (button) {
				if (button.getAttribute("data-bs-toggle") !== null) {
					button.addEventListener("click", function () {
						var target;
						if (button.getAttribute("data-bs-toggle") === "dropdown") {
							toggleDropdown(button, mapToolsMenu);
						} else if (button.getAttribute("data-bs-toggle") === "modal") {
							target = findTargetBySelectorId(button.getAttribute("data-bs-target"));
							if (target) {
								openModal(target);
							}
						}
					}, false);
				}
				if (button.getAttribute("data-bs-dismiss") === "modal") {
					button.addEventListener("click", function () {
						var target = languageDialog;
						if (target) {
							closeModal(target);
						}
					}, false);
				}
			}(buttons[i]));
		}
	}

	function setRouteModeActive(input) {
		var labels = document.getElementsByTagName("label");
		var i;

		for (i = 0; i < labels.length; i++) {
			if (labels[i].getAttribute("for") === input.id) {
				labels[i].classList.add("route-mode-active");
			} else {
				labels[i].classList.remove("route-mode-active");
			}
		}
	}

	function bindRoutingControls() {
		var modes = document.getElementsByName("routing_mode");
		var i;

		console.log("map-route-modes-" + modes.length);

		for (i = 0; i < modes.length; i++) {
			modes[i].addEventListener("click", function () {
				if (this.checked) {
					setRouteModeActive(this);
					setText(status, "Route mode " + this.value);
					console.log("map-route-mode-" + this.value);
				}
			}, false);
		}

		document.getElementById("reverse-route").addEventListener("click", function () {
			var from = routeFrom.value;
			routeFrom.value = routeTo.value;
			routeTo.value = from;
			setText(status, "Route reversed");
			console.log("map-route-reversed");
		}, false);

		document.getElementById("route-loader-toggle").addEventListener("click", function () {
			mapLoader.removeAttribute("hidden");
			setText(status, "Route loader visible");
			console.log("map-loader-visible");
		}, false);
	}

	function countRouteMarkerSvg() {
		var markers = document.getElementsByClassName("routing-marker");
		var count = 0;
		var i;

		for (i = 0; i < markers.length; i++) {
			if (markers[i].getElementsByTagName("svg").length === 1) {
				count++;
			}
		}

		return count;
	}

	function countElementsWithDisabledAttribute(tagName) {
		var nodes = document.getElementsByTagName(tagName);
		var count = 0;
		var i;

		for (i = 0; i < nodes.length; i++) {
			if (nodes[i].hasAttribute("disabled")) {
				count++;
			}
		}

		return count;
	}

	function countRoutingLabels() {
		var labels = document.getElementsByTagName("label");
		var count = 0;
		var i;

		for (i = 0; i < labels.length; i++) {
			if (String(labels[i].getAttribute("for")).indexOf("routing-mode-") === 0) {
				count++;
			}
		}

		return count;
	}

	function countDraggableRouteMarkers() {
		var markers = document.getElementsByClassName("routing-marker");
		var count = 0;
		var i;

		for (i = 0; i < markers.length; i++) {
			if (markers[i].getAttribute("draggable") === "true" &&
					markers[i].getAttribute("data-type") !== null) {
				count++;
			}
		}

		return count;
	}

	function collectControlButtons() {
		var buttons = document.getElementsByTagName("button");
		var found = [];
		var button;
		var i;

		for (i = 0; i < buttons.length; i++) {
			button = buttons.item ? buttons.item(i) : buttons[i];
			if (button && button.matches(".control-button") &&
					button.closest(".leaflet-control")) {
				found.push(button);
			}
		}

		return found;
	}

	function setActiveControl(button) {
		var active = document.getElementsByClassName("control-button-active");
		var i;
		for (i = 0; i < active.length; i++) {
			active[i].classList.remove("control-button-active");
		}
		button.classList.add("control-button-active");
	}

	function handleControlButton(button) {
		setActiveControl(button);
		if (button.getAttribute("data-action") === "pan") {
			offsetX += parseInt(button.getAttribute("data-dx"), 10);
			offsetY += parseInt(button.getAttribute("data-dy"), 10);
			updateTransform();
			console.log("map-pan");
			console.log("map-pan-button");
		} else if (button.getAttribute("data-action") === "zoom") {
			if (button.id === "map-zoom-in") {
				zoomBy(1, "map-zoom-in");
			} else {
				zoomBy(-1, "map-zoom-out");
			}
		} else if (button.getAttribute("data-action") === "edit") {
			setText(status, "Map edit action opened");
			console.log("map-edit-open");
		}
	}

	function stopControlDrag(evt) {
		if (evt && evt.stopPropagation) {
			evt.stopPropagation();
		}
	}

	function beginDrag(evt) {
		if (evt && evt.button !== 0) {
			return;
		}
		dragging = true;
		dragStartX = evt ? evt.clientX : 0;
		dragStartY = evt ? evt.clientY : 0;
		dragOriginX = offsetX;
		dragOriginY = offsetY;
		map.classList.add("leaflet-dragging");
		setText(status, "Dragging map");
		if (evt && evt.preventDefault) {
			evt.preventDefault();
		}
		console.log("map-drag-start");
	}

	function dragMap(evt) {
		if (!dragging) {
			return;
		}
		offsetX = dragOriginX + ((evt ? evt.clientX : dragStartX) - dragStartX);
		offsetY = dragOriginY + ((evt ? evt.clientY : dragStartY) - dragStartY);
		updateTransform();
		if (evt && evt.preventDefault) {
			evt.preventDefault();
		}
		console.log("map-drag-move");
	}

	function endDrag(evt) {
		if (!dragging) {
			return;
		}
		dragging = false;
		map.classList.remove("leaflet-dragging");
		setText(status, "Map drag finished");
		if (evt && evt.preventDefault) {
			evt.preventDefault();
		}
		console.log("map-pan");
		console.log("map-drag-end");
	}

	function wheelZoom(evt) {
		var deltaY = 0;
		if (evt) {
			if (typeof evt.deltaY === "number") {
				deltaY = evt.deltaY;
			} else if (typeof evt.wheelDelta === "number") {
				deltaY = -evt.wheelDelta;
			}
		}
		console.log("map-wheel-event-" + deltaY);
		if (deltaY === 0) {
			console.log("map-wheel-zero");
			return;
		}
		if (evt && evt.preventDefault) {
			evt.preventDefault();
		}
		zoomBy(deltaY < 0 ? 1 : -1,
			deltaY < 0 ? "map-wheel-zoom-in" : "map-wheel-zoom-out");
	}

	function updateSearch() {
		var query = document.getElementById("map-query").value;
		var params = new URLSearchParams({ q: query, z: zoom });
		nearby.innerHTML = "";
		["Bench Cafe", "Transit Hall", "Renderer Plaza", "Script Market"].forEach(function (name, index) {
			var item = document.createElement("li");
			item.append(name + " result for " + params.get("q") + " #" + (index + 1));
			nearby.append(item);
		});
		map.setAttribute("data-query", params.toString());
		setText(status, "Map search updated");
		console.log("map-search-updated");
	}

	function bindMarker(id, kind) {
		document.getElementById(id).addEventListener("click", function () {
			setText(popup, kind + " marker selected: " + this.textContent);
			setText(status, "Marker popup updated");
			this.classList.toggle("active", true);
			contextMenu.classList.toggle("d-none", false);
			console.log("map-marker-popup");
		}, false);
	}

	function bindLayer(id, cls, label) {
		var input = document.getElementById(id);
		var labelNode = input.parentNode;
		function applyLayer(visible) {
			input.checked = visible;
			map.classList.toggle(cls, visible);
			setText(status, label + " layer " + (visible ? "enabled" : "disabled"));
			console.log("map-layer-" + label.toLowerCase() + "-" + (visible ? "on" : "off"));
		}

		input.addEventListener("click", function () {
			applyLayer(this.checked);
		}, false);
		labelNode.addEventListener("click", function (evt) {
			if (evt && evt.target === input) {
				return;
			}
			applyLayer(!input.checked);
		}, false);
	}

	function probeMapFeature(name, supported, missing, fn) {
		try {
			(supported && fn() ? supported : missing).push(name);
		} catch (err) {
			missing.push(name);
		}
	}

	function logMapFeatureGaps() {
		var supported = [];
		var missing = [];

		probeMapFeature("selector-descendant", supported, missing, function () {
			return document.querySelectorAll(".leaflet-control .control-button").length === 7;
		});
		probeMapFeature("selector-attribute", supported, missing, function () {
			return document.querySelectorAll("button[data-action]").length === 7 &&
				document.querySelectorAll("link[type=\"application/atom+xml\"]").length === 1 &&
				document.querySelectorAll("[data-language-code]").length >= 3 &&
				document.querySelectorAll("button[data-bs-target$=\"_edit\"]").length === 1;
		});
		probeMapFeature("osm-modal-dropdown-tags", supported, missing, function () {
			return countElementsWithAttribute("button", "data-bs-toggle", "modal") === 1 &&
				countElementsWithAttribute("button", "data-bs-toggle", "dropdown") === 1 &&
				countElementsWithAttribute("button", "data-bs-dismiss", "modal") === 2 &&
				countElementsWithAttribute("turbo-frame", "loading", "lazy") === 1;
		});
		probeMapFeature("osm-routing-tags", supported, missing, function () {
			return document.querySelectorAll("select optgroup option").length >= 3 &&
				countRoutingLabels() === 3 &&
				countDraggableRouteMarkers() === 2 &&
				countRouteMarkerSvg() === 2;
		});
		probeMapFeature("disabled-attributes", supported, missing, function () {
			return countElementsWithDisabledAttribute("option") >= 2 &&
				countElementsWithDisabledAttribute("input") === 1;
		});
		probeMapFeature("computed-custom-property", supported, missing, function () {
			var value = window.getComputedStyle(document.documentElement)
				.getPropertyValue("--bs-breakpoint-md");
			return String(value).replace(/\s+/g, "") === "768px";
		});
		probeMapFeature("geometry-rect", supported, missing, function () {
			var rect = map.getBoundingClientRect();
			return typeof rect.left === "number" && typeof rect.height === "number";
		});
		probeMapFeature("dataset-read", supported, missing, function () {
			return map.dataset.zoom === "12";
		});
		probeMapFeature("element-tag-local-name", supported, missing, function () {
			var section = document.createElement("section");

			return document.documentElement.localName === "html" &&
				document.documentElement.tagName === "HTML" &&
				section.localName === "section" &&
				section.tagName === "SECTION";
		});
		probeMapFeature("event-dispatch", supported, missing, function () {
			var seen = false;
			var evt = document.createEvent("Event");
			map.addEventListener("map-feature-probe", function (event) {
				seen = true;
				event.preventDefault();
			}, false);
			evt.initEvent("map-feature-probe", true, true);
			return map.dispatchEvent(evt) === false && seen;
		});
		probeMapFeature("document-visibility", supported, missing, function () {
			return document.hidden === false &&
				document.visibilityState === "visible";
		});
		probeMapFeature("document-active-element", supported, missing, function () {
			return document.activeElement === document.body;
		});
		probeMapFeature("document-has-focus", supported, missing, function () {
			return document.hasFocus() === true;
		});
		probeMapFeature("document-metadata", supported, missing, function () {
			return document.URL.indexOf("practical-map.html") >= 0 &&
				document.documentURI === document.URL &&
				document.contentType === "text/html" &&
				document.characterSet.length > 0 &&
				document.inputEncoding === document.characterSet;
		});
		probeMapFeature("navigator-runtime-hints", supported, missing, function () {
			return navigator.onLine === true &&
				navigator.language === "en-US" &&
				navigator.languages &&
				navigator.languages.length >= 2 &&
				navigator.languages[0] === "en-US" &&
				navigator.hardwareConcurrency >= 1 &&
				navigator.maxTouchPoints === 0;
		});
		probeMapFeature("document-import-adopt-node", supported, missing, function () {
			var source = document.createElement("div");
			var child = document.createElement("span");
			child.appendChild(document.createTextNode("imported"));
			source.appendChild(child);

			var imported = document.importNode(source, true);
			var adopted = document.adoptNode(document.createElement("strong"));
			return imported !== source &&
				imported.childNodes.length === 1 &&
				imported.firstChild.firstChild.nodeValue === "imported" &&
				adopted.ownerDocument === document;
		});
		probeMapFeature("document-current-script", supported, missing, function () {
			return document.currentScript === null;
		});
		probeMapFeature("node-is-connected", supported, missing, function () {
			var detached = document.createElement("span");
			var child = document.createElement("em");
			var attached;

			detached.appendChild(child);
			map.appendChild(detached);
			attached = map.isConnected === true &&
				detached.isConnected === true &&
				child.isConnected === true;
			map.removeChild(detached);

			return attached &&
				detached.isConnected === false &&
				child.isConnected === false;
		});
		probeMapFeature("node-get-root", supported, missing, function () {
			var detached = document.createElement("div");
			var child = document.createElement("span");

			detached.appendChild(child);
			return map.getRootNode().nodeType === 9 &&
				child.getRootNode().nodeType === 1 &&
				child.getRootNode().nodeName === "DIV" &&
				detached.getRootNode().nodeName === "DIV";
		});
		probeMapFeature("replace-children", supported, missing, function () {
			var holder = document.createElement("button");
			var old = document.createElement("span");
			var icon = document.createElement("i");

			old.appendChild(document.createTextNode("Old"));
			icon.className = "maplibregl-ctrl-icon fs-5";
			holder.appendChild(old);
			holder.replaceChildren("Layer ", icon);

			return holder.childNodes.length === 2 &&
				holder.firstChild.nodeValue === "Layer " &&
				holder.firstElementChild.className === "maplibregl-ctrl-icon fs-5" &&
				old.parentNode === null;
		});
		probeMapFeature("form-submit-methods", supported, missing, function () {
			var form = document.createElement("form");
			var submitSeen = false;
			var resetSeen = false;
			var requestResult;
			var submitResult;
			var resetResult;

			form.addEventListener("submit", function (event) {
				submitSeen = event.bubbles === true &&
					event.cancelable === true;
				event.preventDefault();
			}, false);
			form.addEventListener("reset", function (event) {
				resetSeen = event.bubbles === true &&
					event.cancelable === true;
				event.preventDefault();
			}, false);

			requestResult = form.requestSubmit();
			submitResult = form.submit();
			resetResult = form.reset();

			return submitSeen &&
				resetSeen &&
				form.checkValidity() === true &&
				form.reportValidity() === true &&
				requestResult === undefined &&
				submitResult === undefined &&
				resetResult === undefined;
		});
		probeMapFeature("scroll-into-view", supported, missing, function () {
			return map.scrollIntoView({ block: "center", behavior: "auto" }) === undefined;
		});
		probeMapFeature("document-title", supported, missing, function () {
			var original = document.title;
			var updated = original + " Updated";
			document.title = updated;
			var ok = original.indexOf("Slippy Map") >= 0 &&
				document.title === updated;
			document.title = original;
			return ok && document.title === original;
		});
		probeMapFeature("formdata-basic", supported, missing, function () {
			var form = document.createElement("form");
			var query = document.createElement("input");
			var hidden = document.createElement("input");
			var checked = document.createElement("input");
			var skipped = document.createElement("input");
			var select = document.createElement("select");
			var option = document.createElement("option");
			query.setAttribute("name", "q");
			query.setAttribute("value", "coffee");
			hidden.setAttribute("name", "bbox");
			hidden.setAttribute("type", "hidden");
			hidden.setAttribute("value", "-122,37,-121,38");
			checked.setAttribute("name", "amenity");
			checked.setAttribute("type", "checkbox");
			checked.setAttribute("checked", "checked");
			checked.setAttribute("value", "cafe");
			skipped.setAttribute("name", "skip");
			skipped.setAttribute("type", "checkbox");
			skipped.setAttribute("value", "no");
			select.setAttribute("name", "mode");
			option.setAttribute("value", "walk");
			option.setAttribute("selected", "selected");
			option.appendChild(document.createTextNode("Walk"));
			select.appendChild(option);
			form.appendChild(query);
			form.appendChild(hidden);
			form.appendChild(checked);
			form.appendChild(skipped);
			form.appendChild(select);

			var data = new FormData(form);
			data.append("tag", "map");
			data.set("q", "bakery");
			var seen = [];
			data.forEach(function (value, key) {
				seen.push(key + "=" + value);
			});
			return data.get("q") === "bakery" &&
				data.get("amenity") === "cafe" &&
				data.get("mode") === "walk" &&
				data.get("skip") === null &&
				data.has("bbox") &&
				data.getAll("tag").length === 1 &&
				Array.from(data.entries()).length === seen.length;
		});
		probeMapFeature("window-scroll-api", supported, missing, function () {
			var scrollToResult = window.scrollTo(0, 0);
			var scrollByResult = window.scrollBy(0, 0);
			return window.scrollX === 0 &&
				window.pageXOffset === 0 &&
				window.scrollY === 0 &&
				window.pageYOffset === 0 &&
				scrollToResult === undefined &&
				scrollByResult === undefined;
		});
		probeMapFeature("window-match-media", supported, missing, function () {
			var wide = window.matchMedia("(min-width: 1px)");
			var narrow = window.matchMedia("(max-width: 1px)");

			return wide.media === "(min-width: 1px)" &&
				wide.matches === true &&
				narrow.matches === false &&
				typeof wide.addListener === "function" &&
				typeof wide.removeListener === "function" &&
				typeof wide.addEventListener === "function" &&
				typeof wide.removeEventListener === "function" &&
				wide.dispatchEvent(document.createEvent("Event")) === true;
		});
		probeMapFeature("abort-controller", supported, missing, function () {
			var controller = new AbortController();
			controller.abort();
			return controller.signal.aborted;
		});
		probeMapFeature("abort-signal-timeout", supported, missing, function () {
			var signal = AbortSignal.timeout(25);
			return signal &&
				signal.aborted === false &&
				typeof signal.addEventListener === "function";
		});
		probeMapFeature("intersection-observer", supported, missing, function () {
			var observer = new IntersectionObserver(function () {});
			observer.observe(map);
			observer.disconnect();
			return true;
		});

		console.log("map-feature-supported " + supported.join(","));
		console.log("map-feature-gaps " + (missing.join(",") || "none"));
	}

	for (i = 0; i < 36; i++) {
		var x = i % 6;
		var y = Math.floor(i / 6);
		var kind = (x + y) % 5 === 0 ? "water" : ((x * y) % 4 === 0 ? "park" : "city");
		addTile(x, y, kind);
	}

	controls = collectControlButtons();
	for (i = 0; i < controls.length; i++) {
		(function (button) {
			button.addEventListener("mousedown", stopControlDrag, false);
			button.addEventListener("click", function () {
				handleControlButton(button);
			}, false);
		}(controls[i]));
	}
	console.log("map-controls-bound-" + controls.length);

	document.getElementById("map-search-button").addEventListener("click", updateSearch, false);
	bindBootstrapLikeControls();
	bindRoutingControls();
	map.addEventListener("mousedown", beginDrag, false);
	map.addEventListener("wheel", wheelZoom, false);
	document.addEventListener("mousemove", dragMap, false);
	document.addEventListener("mouseup", endDrag, false);
	document.addEventListener("mouseleave", endDrag, false);
	bindMarker("marker-transit", "Transit");
	bindMarker("marker-food", "Food");
	bindMarker("marker-alert", "Alert");
	bindLayer("layer-transit", "Transit", "Transit");
	bindLayer("layer-food", "Food", "Food");
	bindLayer("layer-alerts", "Alerts", "Alert");
	addControlIcon();
	updateSearch();
	logMapFeatureGaps();

	if (mapToolsButton && mapToolsMenu && languageDialog &&
			document.getElementsByTagName("turbo-frame").length >= 2 &&
			document.getElementsByTagName("optgroup").length >= 1 &&
			countRouteMarkerSvg() === 2) {
		console.log("map-osm-tags-ready");
	}

	if (map.matches(".leaflet-container") &&
			document.querySelector(".leaflet-marker-icon").closest(".leaflet-container") === map &&
			((mapUi.children && mapUi.children.length > 0) || mapUi.childNodes.length > 0) &&
			window.matchMedia("(min-width: 320px)").matches) {
		console.log("map-dom-compat-ready");
	}

	updateTransform();
	console.log("map-inline-svg-count-" + svgCount);
	console.log("map-tile-count-" + tileCount);
	console.log("practical-map-ready");
}());
