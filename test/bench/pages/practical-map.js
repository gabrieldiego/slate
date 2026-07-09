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
	var controls = [];
	var svgCount = document.getElementsByTagName("svg").length;
	var tileCount = 0;
	var offsetX = 0;
	var offsetY = 0;
	var zoom = parseInt(map.getAttribute("data-zoom"), 10) || 12;
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
			setText(centerText, offsetX + "," + offsetY);
			setText(zoomText, zoom);
			setText(status, "Map center " + offsetX + "," + offsetY + " zoom " + zoom);
			console.log("map-frame-" + offsetX + "," + offsetY);
		});
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
		} else {
			zoom += parseInt(button.getAttribute("data-dz"), 10);
			updateTransform();
			if (button.id === "map-zoom-in") {
				console.log("map-zoom-in");
			} else {
				console.log("map-zoom-out");
			}
		}
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
			return document.querySelectorAll(".leaflet-control .control-button").length === 6;
		});
		probeMapFeature("selector-attribute", supported, missing, function () {
			return document.querySelectorAll("button[data-action]").length === 6;
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
		probeMapFeature("abort-controller", supported, missing, function () {
			var controller = new AbortController();
			controller.abort();
			return controller.signal.aborted;
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
			button.addEventListener("click", function () {
				handleControlButton(button);
			}, false);
		}(controls[i]));
	}
	console.log("map-controls-bound-" + controls.length);

	document.getElementById("map-search-button").addEventListener("click", updateSearch, false);
	bindMarker("marker-transit", "Transit");
	bindMarker("marker-food", "Food");
	bindMarker("marker-alert", "Alert");
	bindLayer("layer-transit", "Transit", "Transit");
	bindLayer("layer-food", "Food", "Food");
	bindLayer("layer-alerts", "Alerts", "Alert");
	addControlIcon();
	updateSearch();
	logMapFeatureGaps();

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
