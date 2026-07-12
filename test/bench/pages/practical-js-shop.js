(function () {
	var data = JSON.parse('[' +
		'{"name":"Bench Camera","price":129,"image":"assets/product-camera.jpg","rating":4.4,"reviews":318,"badge":"Deal","pickup":true,"shipping":"Free 2-day","variants":["Black","Silver"],"stock":"Only 4 left"},' +
		'{"name":"Compact Keyboard","price":74,"image":"assets/product-keyboard.jpg","rating":4.7,"reviews":851,"badge":"Top rated","pickup":true,"shipping":"Free shipping","variants":["ANSI","ISO"],"stock":"In stock"},' +
		'{"name":"Low Power Monitor","price":219,"image":"assets/product-monitor.jpg","rating":4.2,"reviews":126,"badge":"Energy saver","pickup":false,"shipping":"Arrives Friday","variants":["24 inch","27 inch"],"stock":"Ships soon"}' +
	']');
	var products = document.getElementById("shop-products");
	var status = document.getElementById("cart-status");
	var total = document.getElementById("cart-total");
	var fulfillment = document.getElementById("cart-fulfillment");
	var recommendations = document.getElementById("shop-recommendations");
	var reviews = document.getElementById("shop-reviews");
	var drawer = document.getElementById("checkout-drawer");
	var cart = [];
	var wishlist = [];
	var compare = [];
	var discount = 0;

	function setText(node, text) {
		node.innerHTML = "";
		node.appendChild(document.createTextNode(text));
	}

	function renderCart() {
		var subtotal = cart.reduce(function (sum, item) {
			return sum + item.price;
		}, 0);
		var finalTotal = Math.max(0, subtotal - discount);
		setText(status, cart.length + " bench products in cart");
		setText(total, "Cart total: $" + finalTotal);
		setText(fulfillment, cart.length ? "Pickup available for selected bench products" : "Choose pickup or shipping.");
	}

	function renderProducts(source) {
		products.innerHTML = "";
		source.forEach(function (item) {
			var card = document.createElement("article");
			var image = document.createElement("img");
			var heading = document.createElement("h2");
			var rating = document.createElement("p");
			var price = document.createElement("p");
			var stock = document.createElement("p");
			var tags = document.createElement("p");
			var variants = document.createElement("p");
			var addButton = document.createElement("button");
			var wishButton = document.createElement("button");
			var compareBox = document.createElement("label");
			var variantText = item.variants.join(" / ");

			card.className = "product-card";
			image.setAttribute("src", item.image);
			image.setAttribute("width", "120");
			image.setAttribute("height", "80");
			image.setAttribute("alt", item.name);
			heading.appendChild(document.createTextNode(item.name));
			rating.appendChild(document.createTextNode("Rating " + item.rating + " from " + item.reviews + " reviews"));
			price.className = "sale";
			price.appendChild(document.createTextNode("Sale price $" + item.price));
			stock.className = "stock";
			stock.appendChild(document.createTextNode(item.stock + " / " + item.shipping));
			tags.innerHTML = '<span class="badge">' + item.badge + '</span>' +
				'<span class="badge">' + (item.pickup ? "Pickup today" : "Ship only") + '</span>';
			variants.className = "variant-row";
			variants.appendChild(document.createTextNode("Variants: " + variantText));
			addButton.setAttribute("type", "button");
			addButton.appendChild(document.createTextNode("Add " + item.name));
			addButton.onclick = function () {
				cart.push(item);
				renderCart();
				console.log("shop-added-" + item.name.replace(/ /g, "-").toLowerCase());
			};
			wishButton.setAttribute("type", "button");
			wishButton.appendChild(document.createTextNode("Save " + item.name));
			wishButton.onclick = function () {
				wishlist.push(item.name);
				setText(status, "Saved to wishlist: " + item.name);
				console.log("shop-wishlist-updated");
			};
			compareBox.innerHTML = '<input type="checkbox"> Compare ' + item.name;
			compareBox.onclick = function () {
				compare.push(item.name);
				setText(status, "Compare list has " + compare.length + " products");
			};

			card.appendChild(image);
			card.appendChild(heading);
			card.appendChild(rating);
			card.appendChild(price);
			card.appendChild(stock);
			card.appendChild(tags);
			card.appendChild(variants);
			card.appendChild(addButton);
			card.appendChild(document.createTextNode(" "));
			card.appendChild(wishButton);
			card.appendChild(document.createElement("br"));
			card.appendChild(compareBox);
			products.appendChild(card);
		});
	}

	function renderRecommendations() {
		recommendations.innerHTML = "";
		data.forEach(function (item) {
			var rec = document.createElement("button");
			rec.setAttribute("type", "button");
			rec.appendChild(document.createTextNode("Recommended " + item.name));
			rec.onclick = function () {
				setText(status, "Recommendation selected: " + item.name);
			};
			recommendations.appendChild(rec);
			recommendations.appendChild(document.createTextNode(" "));
		});
	}

	function renderReviews() {
		var snippets = [
			"Fast pickup and clear product details.",
			"Variant selector was easy to scan.",
			"Cart summary stayed visible during checkout."
		];
		reviews.innerHTML = "";
		snippets.forEach(function (text, index) {
			var quote = document.createElement("blockquote");
			quote.appendChild(document.createTextNode("Review " + (index + 1) + ": " + text));
			reviews.appendChild(quote);
		});
	}

	function verifyMarketplaceSelectors() {
		var heading = document.querySelector(".shop-marketplace > h2");
		var cards = document.querySelectorAll(".shop-marketplace > .marketplace-card");
		var firstNav = document.querySelector("#marketplace-nav > li[data-track='electronics']");

		if (heading && cards.length === 3 && firstNav) {
			console.log("shop-selector-child-combinator");
		}
	}

	function verifyMarketplaceEvents() {
		window.addEventListener("shop-marketplace-event", function (event) {
			if (event.detail && event.detail.source === "marketplace") {
				console.log("shop-custom-event-dispatched");
			}
		});
		window.dispatchEvent(new CustomEvent("shop-marketplace-event", {
			detail: window.shopInlinePayload || { source: "marketplace" }
		}));
	}

	function verifyMarketplaceClassFields() {
		class MarketplaceRequest {
			xhr;
			route;
			async;
			static MAX_TIMEOUT;

			constructor(route) {
				this.route = route;
				this.async = true;
			}

			attach(xhr) {
				this.xhr = xhr;
				return this.route + ":" + this.xhr;
			}
		}

		MarketplaceRequest.MAX_TIMEOUT = 5000;
		var request = new MarketplaceRequest("/shop/search");
		if (request.async &&
				request.attach("inventory") === "/shop/search:inventory" &&
				MarketplaceRequest.MAX_TIMEOUT === 5000) {
			console.log("shop-class-fields");
		}
	}

	function verifyMarketplaceMutationObserver() {
		var target = document.getElementById("shop-products");
		var observer = new MutationObserver(function () {});
		var records;

		observer.observe(target, {
			childList: true,
			subtree: true
		});
		records = observer.takeRecords();
		observer.disconnect();

		if (records && records.length === 0) {
			console.log("shop-mutation-observer");
		}
	}

	document.getElementById("cart-discount").onclick = function () {
		discount = 20;
		renderCart();
		setText(status, "Bench discount applied");
		console.log("shop-discount-applied");
	};
	document.getElementById("cart-checkout").onclick = function () {
		drawer.innerHTML = "<h3>Checkout Drawer</h3><p>Cart drawer opened with " + cart.length + " products.</p>";
		console.log("shop-checkout-drawer");
	};
	document.getElementById("shop-apply-filters").onclick = function () {
		var pickupOnly = document.getElementById("shop-pickup").checked;
		var filtered = data.filter(function (item) {
			return !pickupOnly || item.pickup;
		});
		renderProducts(filtered);
		setText(status, "Applied shop filters");
		console.log("shop-filters-applied");
	};
	document.getElementById("shop-search-button").onclick = function () {
		var query = document.getElementById("shop-query").value.toLowerCase();
		var found = data.filter(function (item) {
			return item.name.toLowerCase().indexOf(query) !== -1 ||
				item.badge.toLowerCase().indexOf(query) !== -1;
		});
		renderProducts(found);
		setText(status, "Shop search returned " + found.length + " products");
		console.log("shop-search-complete");
	};

	renderProducts(data);
	renderRecommendations();
	renderReviews();
	verifyMarketplaceSelectors();
	verifyMarketplaceEvents();
	verifyMarketplaceClassFields();
	verifyMarketplaceMutationObserver();
	setText(status, "Practical Bench JS shop ready");
	console.log("practical-js-shop-ready");
}());
