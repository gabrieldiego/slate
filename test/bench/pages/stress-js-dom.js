(function () {
	var grid = document.getElementById("stress-js-grid");
	var status = document.getElementById("stress-js-status");
	var expand = document.getElementById("stress-js-expand");
	var words = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"];
	var checksum = 0;
	var i;

	function setText(node, text) {
		node.innerHTML = "";
		node.appendChild(document.createTextNode(text));
	}

	function churnFinalizerCandidates() {
		var bucket = document.createElement("div");
		var j;

		bucket.className = "stress-card finalizer-churn";
		document.body.appendChild(bucket);
		for (j = 0; j < 260; j++) {
			var item = document.createElement("span");
			var child = document.createElement("em");
			var attrs;
			var list;
			var clone;

			item.className = "finalizer-item item-" + (j % 11);
			item.setAttribute("data-finalizer", "candidate-" + j);
			child.appendChild(document.createTextNode("child " + j));
			item.appendChild(child);
			bucket.appendChild(item);

			attrs = item.attributes;
			if (attrs && attrs.length) {
				checksum += attrs.length;
			}
			if (item.classList) {
				item.classList.add("touched");
				item.classList.remove("touched");
			}
			list = bucket.getElementsByTagName("span");
			if (list && list.length && list.item) {
				list.item(0);
			}
			clone = item.cloneNode(false);
			bucket.appendChild(clone);
			bucket.removeChild(clone);
			bucket.removeChild(item);
		}
		document.body.removeChild(bucket);
		console.log("stress-js-finalizer-churn");
	}

	for (i = 0; i < 180; i++) {
		var card = document.createElement("article");
		var title = document.createElement("strong");
		var body = document.createElement("p");
		var tag = words[i % words.length].replace(/[aeiou]/g, "x");

		checksum += (i * 17) % 29;
		card.className = "stress-card bucket-" + (i % 9);
		card.setAttribute("title", "Stress generated card " + i);
		title.appendChild(document.createTextNode("Stress JS card " + i));
		body.appendChild(document.createTextNode("Generated tag " + tag + " checksum " + checksum));
		card.appendChild(title);
		card.appendChild(body);
		grid.appendChild(card);
	}

	churnFinalizerCandidates();

	expand.onclick = function () {
		var summary = document.createElement("p");
		summary.appendChild(document.createTextNode("Stress JS expanded summary checksum " + checksum));
		document.body.appendChild(summary);
		setText(status, "Stress JS summary expanded");
		console.log("stress-js-summary-expanded");
	};

	setText(status, "Stress JS generated 180 cards");
	console.log("stress-js-dom-ready");
}());
