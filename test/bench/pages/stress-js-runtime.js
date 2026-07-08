(function () {
	var results = document.getElementById("runtime-results");
	var status = document.getElementById("runtime-status");
	var values = [];
	var i;
	var descriptorTarget = {};

	function addRow(name, value) {
		var row = document.createElement("tr");
		var label = document.createElement("td");
		var cell = document.createElement("td");
		label.appendChild(document.createTextNode(name));
		cell.appendChild(document.createTextNode(String(value)));
		row.appendChild(label);
		row.appendChild(cell);
		results.appendChild(row);
	}

	for (i = 0; i < 240; i++) {
		values.push((i * i + 3 * i) % 97);
	}

	Object.defineProperty(descriptorTarget, "benchValue", {
		get: function () {
			return values.length;
		}
	});

	var mapped = values.map(function (value) {
		return value + 1;
	});
	var filtered = mapped.filter(function (value) {
		return value % 3 === 0;
	});
	var reduced = filtered.reduce(function (sum, value) {
		return sum + value;
	}, 0);
	var encoded = JSON.stringify({
		total: reduced,
		count: filtered.length,
		date: new Date(1700000000000).getUTCFullYear()
	});
	var parsed = JSON.parse(encoded);
	var regexp = /bench-(\d+)/;
	var match = regexp.exec("runtime bench-42 complete");

	try {
		throw new Error("bench runtime handled");
	} catch (err) {
		addRow("error", err.message);
	}

	addRow("mapped count", mapped.length);
	addRow("filtered count", parsed.count);
	addRow("reduced total", parsed.total);
	addRow("descriptor", descriptorTarget.benchValue);
	addRow("regexp", match ? match[1] : "missing");
	status.innerHTML = "Stress JS runtime complete";
	console.log("stress-js-runtime-ready");
}());
