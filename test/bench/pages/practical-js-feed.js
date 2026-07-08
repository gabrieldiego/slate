(function () {
	var items = [
		{ title: "Inbox 01: renderer memory baseline", kind: "unread", saved: true },
		{ title: "Inbox 02: CSS layout review", kind: "read", saved: false },
		{ title: "Inbox 03: JavaScript compatibility pass", kind: "unread", saved: false },
		{ title: "Inbox 04: image decoding notes", kind: "read", saved: true }
	];
	var list = document.getElementById("feed-list");
	var status = document.getElementById("feed-status");
	var count = document.getElementById("feed-count");
	var widget = document.getElementById("feed-widget");
	var activeFilter = "all";

	function setText(node, text) {
		node.innerHTML = "";
		node.appendChild(document.createTextNode(text));
	}

	function visible(item) {
		if (activeFilter === "unread") {
			return item.kind === "unread";
		}
		if (activeFilter === "saved") {
			return item.saved;
		}
		return true;
	}

	function render() {
		var rendered = 0;
		list.innerHTML = "";
		items.forEach(function (item, index) {
			if (!visible(item)) {
				return;
			}
			var article = document.createElement("article");
			var heading = document.createElement("h2");
			var meta = document.createElement("p");
			var button = document.createElement("button");

			article.className = "feed-item";
			heading.appendChild(document.createTextNode(item.title));
			meta.appendChild(document.createTextNode("Status " + item.kind + ", saved " + item.saved));
			button.setAttribute("type", "button");
			button.appendChild(document.createTextNode("Mark Bench Item " + (index + 1)));
			button.onclick = function () {
				item.kind = "read";
				setText(status, "Marked read: " + item.title);
				render();
			};

			article.appendChild(heading);
			article.appendChild(meta);
			article.appendChild(button);
			list.appendChild(article);
			rendered++;
		});
		setText(count, "Rendered bench feed items: " + rendered);
	}

	function filterTo(nextFilter, label) {
		activeFilter = nextFilter;
		setText(status, label + " filter active");
		render();
		console.log("practical-js-feed-filter-" + nextFilter);
	}

	document.getElementById("feed-all").onclick = function () {
		filterTo("all", "All");
	};
	document.getElementById("feed-unread").onclick = function () {
		filterTo("unread", "Unread");
	};
	document.getElementById("feed-saved").onclick = function () {
		filterTo("saved", "Saved");
	};
	document.getElementById("feed-add").onclick = function () {
		items.push({
			title: "Generated bench item " + items.length,
			kind: "unread",
			saved: false
		});
		setText(status, "Generated bench item added");
		render();
		console.log("practical-js-feed-item-added");
	};
	document.getElementById("feed-refresh").onclick = function () {
		setText(widget, "Widget refresh scheduled");
		setTimeout(function () {
			setText(widget, "Async widget refreshed");
			console.log("practical-js-feed-async-refresh");
		}, 20);
	};

	render();
	setText(status, "Practical Bench JS feed ready");
	console.log("practical-js-feed-ready");
}());
