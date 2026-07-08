(function () {
	var title = document.title || "Practical Bench";
	var status = document.createElement("p");

	function setText(node, text) {
		node.innerHTML = "";
		node.appendChild(document.createTextNode(text));
	}

	status.setAttribute("class", "bench-callout");
	status.setAttribute("id", "practical-common-status");
	setText(status, "Practical Common JS ready: " + title);
	if (document.body) {
		document.body.appendChild(status);
	}

	console.log("practical-common-ready " + title);
}());
