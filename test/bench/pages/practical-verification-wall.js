(function () {
	var checkbox = document.getElementById("human-check");
	var button = document.getElementById("continue-button");
	var message = document.getElementById("challenge-message");
	var result = document.getElementById("verification-result");
	var marker = document.getElementById("session-marker");

	function setText(node, text) {
		while (node && node.firstChild) {
			node.removeChild(node.firstChild);
		}
		if (node) {
			node.appendChild(document.createTextNode(text));
		}
	}

	function logChallengeState(name) {
		console.log("verification-wall-" + name);
	}

	setText(marker, "bench-session-ready");
	logChallengeState("detected");

	checkbox.addEventListener("click", function () {
		if (checkbox.checked) {
			button.disabled = false;
			button.setAttribute("aria-disabled", "false");
			setText(message, "Manual verification accepted for this fixture.");
			logChallengeState("checkbox-checked");
		} else {
			button.disabled = true;
			button.setAttribute("aria-disabled", "true");
			setText(message, "Check the box to enable the continuation control.");
			logChallengeState("checkbox-cleared");
		}
	}, false);

	button.addEventListener("click", function () {
		if (button.disabled) {
			logChallengeState("continue-blocked");
			return;
		}
		document.body.classList.add("verification-complete");
		setText(result, "Verification wall passed to local content.");
		logChallengeState("continue-clicked");
	}, false);

	document.querySelector(".fallback-form").addEventListener("submit", function (event) {
		if (event && event.preventDefault) {
			event.preventDefault();
		}
		setText(result, "Fallback verification form submitted.");
		logChallengeState("fallback-submit");
	}, false);

	console.log("practical-verification-wall-ready");
}());
