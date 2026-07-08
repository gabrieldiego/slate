(function () {
	var messages = [
		{ from: "Ada", subject: "Renderer memory baseline", unread: true, starred: true, body: "The latest Max RSS report needs review.", tags: ["bench", "memory"] },
		{ from: "Grace", subject: "CSS compatibility notes", unread: true, starred: false, body: "Selectors and forms need another pass.", tags: ["css", "layout"] },
		{ from: "Linus", subject: "JavaScript test driver", unread: false, starred: false, body: "The selector based Jotter driver is ready.", tags: ["js", "driver"] },
		{ from: "Radia", subject: "Map tile cache idea", unread: false, starred: true, body: "Slippy map pages are useful for cache behavior.", tags: ["map", "cache"] }
	];
	var selected = {};
	var list = document.getElementById("message-list");
	var status = document.getElementById("mail-status");
	var subject = document.getElementById("message-subject");
	var meta = document.getElementById("message-meta");
	var body = document.getElementById("message-body");
	var tags = document.getElementById("message-tags");

	function setText(node, text) {
		node.innerHTML = "";
		node.appendChild(document.createTextNode(text));
	}

	function renderTags(values) {
		tags.innerHTML = "";
		values.forEach(function (value) {
			var tag = document.createElement("span");
			tag.className = "tag";
			tag.appendChild(document.createTextNode(value));
			tags.appendChild(tag);
		});
	}

	function openMessage(index) {
		var message = messages[index];
		message.unread = false;
		setText(subject, message.subject);
		setText(meta, "From " + message.from + (message.starred ? " / starred" : ""));
		setText(body, message.body);
		renderTags(message.tags);
		setText(status, "Opened message " + message.subject);
		console.log("webmail-open-message");
		renderList(messages);
	}

	function renderList(source) {
		list.innerHTML = "";
		source.forEach(function (message, index) {
			var row = document.createElement("button");
			row.setAttribute("type", "button");
			row.className = "mail-row" + (message.unread ? " unread" : "");
			row.innerHTML = "<strong>" + message.subject + "</strong>" + message.from + " / " + message.tags.join(", ");
			row.onclick = function () {
				openMessage(index);
			};
			list.appendChild(row);
		});
	}

	document.getElementById("mail-search-button").onclick = function () {
		var query = document.getElementById("mail-search").value.toLowerCase();
		var filtered = messages.filter(function (message) {
			return message.subject.toLowerCase().indexOf(query) !== -1 ||
				message.body.toLowerCase().indexOf(query) !== -1 ||
				message.tags.join(" ").indexOf(query) !== -1;
		});
		renderList(filtered);
		setText(status, "Search returned " + filtered.length + " messages");
		console.log("webmail-search-complete");
	};

	document.getElementById("mail-select-all").onclick = function () {
		messages.forEach(function (_message, index) {
			selected[index] = true;
		});
		setText(status, "Selected all bench messages");
		console.log("webmail-select-all");
	};

	document.getElementById("mail-archive").onclick = function () {
		setText(status, "Archived selected bench messages");
		console.log("webmail-archive-selected");
	};

	document.getElementById("mail-star").onclick = function () {
		messages.forEach(function (message, index) {
			if (selected[index]) {
				message.starred = true;
			}
		});
		renderList(messages);
		setText(status, "Starred selected bench messages");
	};

	document.getElementById("compose-open").onclick = function () {
		setText(status, "Compose panel focused");
	};

	document.getElementById("compose-send").onclick = function () {
		var sentSubject = document.getElementById("compose-subject").value;
		messages.unshift({
			from: "Me",
			subject: sentSubject,
			unread: false,
			starred: false,
			body: document.getElementById("compose-body").value,
			tags: ["sent", "bench"]
		});
		renderList(messages);
		setText(status, "Sent Bench Mail: " + sentSubject);
		console.log("webmail-send-complete");
	};

	renderList(messages);
	console.log("practical-webmail-ready");
}());
