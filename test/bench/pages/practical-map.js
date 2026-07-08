(function () {
	var grid = document.getElementById("tile-grid");
	var status = document.getElementById("map-status");
	var popup = document.getElementById("map-popup");
	var zoomText = document.getElementById("map-zoom");
	var centerText = document.getElementById("map-center");
	var nearby = document.getElementById("map-nearby");
	var offsetX = 0;
	var offsetY = 0;
	var zoom = 12;
	var i;

	function setText(node, text) {
		node.innerHTML = "";
		node.appendChild(document.createTextNode(text));
	}

	function updateTransform() {
		grid.style.left = offsetX + "px";
		grid.style.top = offsetY + "px";
		setText(centerText, offsetX + "," + offsetY);
		setText(zoomText, zoom);
		setText(status, "Map center " + offsetX + "," + offsetY + " zoom " + zoom);
	}

	function addTile(x, y, kind) {
		var tile = document.createElement("div");
		tile.className = "map-tile tile-" + kind;
		tile.style.left = (x * 128) + "px";
		tile.style.top = (y * 128) + "px";
		tile.appendChild(document.createTextNode("z" + zoom + " / " + x + "," + y));
		grid.appendChild(tile);
	}

	for (i = 0; i < 36; i++) {
		var x = i % 6;
		var y = Math.floor(i / 6);
		var kind = (x + y) % 5 === 0 ? "water" : ((x * y) % 4 === 0 ? "park" : "city");
		addTile(x, y, kind);
	}

	document.getElementById("map-pan-left").onclick = function () {
		offsetX += 48;
		updateTransform();
	};
	document.getElementById("map-pan-right").onclick = function () {
		offsetX -= 48;
		updateTransform();
	};
	document.getElementById("map-pan-up").onclick = function () {
		offsetY += 48;
		updateTransform();
	};
	document.getElementById("map-pan-down").onclick = function () {
		offsetY -= 48;
		updateTransform();
	};
	document.getElementById("map-zoom-in").onclick = function () {
		zoom++;
		updateTransform();
		console.log("map-zoom-in");
	};
	document.getElementById("map-zoom-out").onclick = function () {
		zoom--;
		updateTransform();
		console.log("map-zoom-out");
	};
	document.getElementById("map-search-button").onclick = function () {
		var query = document.getElementById("map-query").value;
		nearby.innerHTML = "";
		["Bench Cafe", "Transit Hall", "Renderer Plaza", "Script Market"].forEach(function (name, index) {
			var item = document.createElement("li");
			item.appendChild(document.createTextNode(name + " result for " + query + " #" + (index + 1)));
			nearby.appendChild(item);
		});
		setText(status, "Map search updated");
		console.log("map-search-updated");
	};

	function bindMarker(id, kind) {
		document.getElementById(id).onclick = function () {
			setText(popup, kind + " marker selected: " + this.innerHTML);
			setText(status, "Marker popup updated");
			console.log("map-marker-popup");
		};
	}
	bindMarker("marker-transit", "Transit");
	bindMarker("marker-food", "Food");
	bindMarker("marker-alert", "Alert");

	document.getElementById("layer-transit").onclick = function () {
		setText(status, "Transit layer toggled");
	};
	document.getElementById("layer-food").onclick = function () {
		setText(status, "Food layer toggled");
	};
	document.getElementById("layer-alerts").onclick = function () {
		setText(status, "Alert layer toggled");
	};

	updateTransform();
	console.log("practical-map-ready");
}());
