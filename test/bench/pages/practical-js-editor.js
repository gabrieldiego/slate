(function () {
	var title = document.getElementById("editor-title");
	var body = document.getElementById("editor-body");
	var previewTitle = document.getElementById("preview-title");
	var previewBody = document.getElementById("preview-body");
	var metrics = document.getElementById("editor-metrics");
	var status = document.getElementById("editor-status");
	var tags = document.getElementById("preview-tags");
	var comments = document.getElementById("editor-comments");
	var progress = document.getElementById("editor-progress");
	var meter = document.getElementById("editor-readiness");
	var canvas = document.getElementById("editor-canvas");
	var staged = false;

	function setText(node, text) {
		node.innerHTML = "";
		node.appendChild(document.createTextNode(text));
	}

	function logStatus(text, token) {
		setText(status, text);
		if (token) {
			console.log(token);
		}
	}

	function countWords(text) {
		var words = text.replace(/^\s+|\s+$/g, "").split(/\s+/);
		return words.length === 1 && words[0] === "" ? 0 : words.length;
	}

	function renderPreview() {
		var bodyText = body.value;
		var rows = [
			["Words", countWords(bodyText)],
			["Characters", bodyText.length],
			["Category", document.getElementById("editor-category").value],
			["Featured", document.getElementById("editor-featured").checked ? "yes" : "no"]
		];

		setText(previewTitle, title.value);
		setText(previewBody, bodyText);
		metrics.innerHTML = "";
		rows.forEach(function (rowData) {
			var row = document.createElement("tr");
			var key = document.createElement("td");
			var value = document.createElement("td");
			setText(key, rowData[0]);
			setText(value, rowData[1]);
			row.appendChild(key);
			row.appendChild(value);
			metrics.appendChild(row);
		});
		progress.value = staged ? 86 : 52;
		meter.value = staged ? 91 : 68;
	}

	function drawChart() {
		var ctx;
		try {
			ctx = canvas.getContext("2d");
		} catch (err) {
			return;
		}
		if (!ctx) {
			return;
		}
		try {
			ctx.fillStyle = "#f8fafc";
			ctx.fillRect(0, 0, 180, 96);
			ctx.strokeStyle = "#304050";
			ctx.strokeRect(4, 4, 172, 88);
			ctx.fillStyle = "#2f6f9f";
			ctx.fillRect(18, 58, 28, 22);
			ctx.fillRect(58, 42, 28, 38);
			ctx.fillRect(98, 30, 28, 50);
			ctx.fillRect(138, 20, 28, 60);
			ctx.beginPath();
			ctx.moveTo(18, 24);
			ctx.lineTo(166, 24);
			ctx.stroke();
		} catch (ignore) {
			/* Canvas coverage is useful when available, but not required. */
		}
	}

	document.getElementById("editor-bold").onclick = function () {
		body.value = body.value + " Bold practical note.";
		renderPreview();
		logStatus("Bold note appended");
	};
	document.getElementById("editor-link").onclick = function () {
		var link = document.createElement("a");
		link.href = "#preview";
		link.appendChild(document.createTextNode("related preview link"));
		previewBody.appendChild(document.createTextNode(" "));
		previewBody.appendChild(link);
		logStatus("Preview link inserted");
	};
	document.getElementById("editor-tag").onclick = function () {
		var tag = document.createElement("span");
		tag.className = "bench-badge";
		tag.setAttribute("data-added", "script");
		tag.appendChild(document.createTextNode("coverage"));
		tags.appendChild(document.createTextNode(" "));
		tags.appendChild(tag);
		logStatus("Tag added");
		console.log("editor-tag-added");
	};
	document.getElementById("editor-render").onclick = function () {
		renderPreview();
		drawChart();
		logStatus("Preview rendered", "editor-preview-rendered");
	};
	document.getElementById("editor-save").onclick = function () {
		staged = true;
		renderPreview();
		logStatus("Draft saved", "editor-draft-saved");
	};
	document.getElementById("editor-stage").onclick = function () {
		staged = true;
		renderPreview();
		logStatus("Draft staged", "editor-draft-staged");
	};
	document.getElementById("editor-publish").onclick = function () {
		logStatus("Publish path checked", "editor-publish-checked");
	};
	document.getElementById("editor-add-comment").onclick = function () {
		var row = document.createElement("p");
		row.className = "comment-row";
		setText(row, "Comment: " + document.getElementById("editor-comment").value);
		comments.appendChild(row);
		logStatus("Bench comment added", "editor-comment-added");
	};

	renderPreview();
	drawChart();
	logStatus("Practical Bench editorial app ready", "practical-js-editor-ready");
}());
