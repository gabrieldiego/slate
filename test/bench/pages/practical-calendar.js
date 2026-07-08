(function () {
	var grid = document.getElementById("calendar-grid");
	var agenda = document.getElementById("agenda-list");
	var status = document.getElementById("calendar-status");
	var events = [
		{ day: 1, title: "Memory review" },
		{ day: 3, title: "CSS bug triage" },
		{ day: 5, title: "JavaScript coverage pass" }
	];
	var weekOffset = 0;

	function setText(node, text) {
		node.innerHTML = "";
		node.appendChild(document.createTextNode(text));
	}

	function render() {
		var day;
		grid.innerHTML = "";
		agenda.innerHTML = "";
		for (day = 0; day < 7; day++) {
			var row = document.createElement("div");
			var cell = document.createElement("div");
			row.className = "calendar-row";
			cell.className = "calendar-cell";
			cell.appendChild(document.createTextNode("Day " + (day + 1 + weekOffset)));
			events.forEach(function (event) {
				if (event.day === day) {
					var pill = document.createElement("span");
					pill.className = "event-pill";
					pill.appendChild(document.createTextNode(event.title));
					cell.appendChild(pill);
				}
			});
			row.appendChild(cell);
			grid.appendChild(row);
		}
		events.forEach(function (event) {
			var item = document.createElement("li");
			item.appendChild(document.createTextNode(event.title + " on day " + (event.day + 1 + weekOffset)));
			agenda.appendChild(item);
		});
	}

	document.getElementById("calendar-prev").onclick = function () {
		weekOffset -= 7;
		render();
		setText(status, "Previous week loaded");
	};
	document.getElementById("calendar-next").onclick = function () {
		weekOffset += 7;
		render();
		setText(status, "Next week loaded");
		console.log("calendar-next-week");
	};
	document.getElementById("calendar-today").onclick = function () {
		weekOffset = 0;
		render();
		setText(status, "Today loaded");
	};
	document.getElementById("event-add").onclick = function () {
		events.push({
			day: events.length % 7,
			title: document.getElementById("event-title").value
		});
		render();
		setText(status, "Added Bench Event");
		console.log("calendar-event-added");
	};

	render();
	console.log("practical-calendar-ready");
}());
