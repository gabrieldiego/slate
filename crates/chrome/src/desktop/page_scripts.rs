/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Browser-owned scripts injected into rendered pages.

pub(crate) const SLATE_TEXT_SELECTION_SCRIPT: &str = r##"
(() => {
  if (window.__slateTextSelectionInstalled) {
    return;
  }
  window.__slateTextSelectionInstalled = true;

  const DRAG_THRESHOLD = 4;
  const RECT_TOLERANCE = 8;
  const MAX_TEXT_NODES = 160;
  const MAX_TEXT_UNITS = 6000;
  let anchorPosition = null;
  let pointerStart = null;
  let selecting = false;
  let contextMenu = null;

  function isEditable(target) {
    const element = target && target.nodeType === Node.ELEMENT_NODE
      ? target
      : target && target.parentElement;
    return Boolean(element && element.closest(
      "input, textarea, select, option, [contenteditable=\"\"], [contenteditable=\"true\"]"
    ));
  }

  function removeContextMenu() {
    if (contextMenu) {
      contextMenu.remove();
      contextMenu = null;
    }
  }

  function selectedText() {
    const selection = window.getSelection && window.getSelection();
    return selection ? String(selection).trim() : "";
  }

  function textNodesUnder(root) {
    const nodes = [];
    let textUnits = 0;
    const walker = document.createTreeWalker(
      root,
      NodeFilter.SHOW_TEXT,
      {
        acceptNode(node) {
          const parent = node.parentElement;
          if (!parent || !node.nodeValue || !node.nodeValue.trim()) {
            return NodeFilter.FILTER_REJECT;
          }
          if (parent.closest("script, style, noscript, input, textarea, select, option")) {
            return NodeFilter.FILTER_REJECT;
          }
          return NodeFilter.FILTER_ACCEPT;
        }
      }
    );

    while (nodes.length < MAX_TEXT_NODES && textUnits < MAX_TEXT_UNITS) {
      const node = walker.nextNode();
      if (!node) {
        break;
      }
      nodes.push(node);
      textUnits += node.nodeValue.length;
    }
    return nodes;
  }

  function candidateTextNodes(element) {
    let current = element;
    while (current && current !== document) {
      const nodes = textNodesUnder(current);
      if (nodes.length > 0) {
        return nodes;
      }
      current = current.parentElement;
    }
    return document.body ? textNodesUnder(document.body) : [];
  }

  function distanceToRect(x, y, rect) {
    const clampedX = Math.max(rect.left, Math.min(rect.right, x));
    const clampedY = Math.max(rect.top, Math.min(rect.bottom, y));
    return Math.hypot(x - clampedX, y - clampedY);
  }

  function rectsNearPoint(range, x, y) {
    const rects = Array.from(range.getClientRects());
    return rects.filter((rect) => (
      rect.width > 0 &&
      rect.height > 0 &&
      y >= rect.top - RECT_TOLERANCE &&
      y <= rect.bottom + RECT_TOLERANCE &&
      x >= rect.left - 80 &&
      x <= rect.right + 80
    ));
  }

  function textPositionFromPoint(x, y) {
    const element = document.elementFromPoint(x, y);
    if (!element) {
      return null;
    }

    let best = null;
    const range = document.createRange();
    for (const node of candidateTextNodes(element)) {
      const value = node.nodeValue || "";
      for (let offset = 0; offset < value.length && offset < MAX_TEXT_UNITS; offset += 1) {
        range.setStart(node, offset);
        range.setEnd(node, offset + 1);
        for (const rect of rectsNearPoint(range, x, y)) {
          const score = distanceToRect(x, y, rect);
          if (!best || score < best.score) {
            const midpoint = rect.left + rect.width / 2;
            best = {
              node,
              offset: x > midpoint ? offset + 1 : offset,
              score
            };
          }
        }
      }
    }
    range.detach();
    return best && { node: best.node, offset: best.offset };
  }

  function applySelection(anchor, focus) {
    const selection = window.getSelection && window.getSelection();
    if (!selection || !anchor || !focus) {
      return false;
    }

    try {
      selection.setBaseAndExtent(anchor.node, anchor.offset, focus.node, focus.offset);
      return true;
    } catch (_) {}

    const range = document.createRange();
    try {
      range.setStart(anchor.node, anchor.offset);
      range.setEnd(focus.node, focus.offset);
    } catch (_) {
      try {
        range.setStart(focus.node, focus.offset);
        range.setEnd(anchor.node, anchor.offset);
      } catch (_) {
        range.detach();
        return false;
      }
    }
    selection.removeAllRanges();
    selection.addRange(range);
    return true;
  }

  function selectAllPageText() {
    const selection = window.getSelection && window.getSelection();
    if (!selection || !document.body) {
      return false;
    }
    selection.removeAllRanges();
    selection.selectAllChildren(document.body);
    return true;
  }

  function copySelectedText() {
    const text = selectedText();
    if (!text) {
      return;
    }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).catch(() => {});
    }
  }

  function showContextMenu(x, y) {
    removeContextMenu();
    const menu = document.createElement("div");
    menu.setAttribute("role", "menu");
    menu.style.position = "fixed";
    menu.style.left = `${x}px`;
    menu.style.top = `${y}px`;
    menu.style.zIndex = "2147483647";
    menu.style.minWidth = "144px";
    menu.style.padding = "5px";
    menu.style.border = "1px solid #b9b5ad";
    menu.style.borderRadius = "6px";
    menu.style.background = "#fffefa";
    menu.style.boxShadow = "0 8px 24px rgba(24, 24, 20, 0.18)";
    menu.style.color = "#242421";
    menu.style.font = "13px system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif";

    const addItem = (label, action) => {
      const item = document.createElement("button");
      item.type = "button";
      item.textContent = label;
      item.style.display = "block";
      item.style.width = "100%";
      item.style.border = "0";
      item.style.borderRadius = "4px";
      item.style.background = "transparent";
      item.style.color = "inherit";
      item.style.font = "inherit";
      item.style.padding = "7px 9px";
      item.style.textAlign = "left";
      item.addEventListener("mouseover", () => {
        item.style.background = "#ebe7df";
      });
      item.addEventListener("mouseout", () => {
        item.style.background = "transparent";
      });
      item.addEventListener("mousedown", (event) => {
        event.preventDefault();
      });
      item.addEventListener("click", (event) => {
        event.preventDefault();
        action();
        removeContextMenu();
      });
      menu.appendChild(item);
    };

    addItem("Copy", copySelectedText);
    addItem("Select All", selectAllPageText);
    addItem("Clear Selection", () => {
      const selection = window.getSelection && window.getSelection();
      if (selection) {
        selection.removeAllRanges();
      }
    });

    document.documentElement.appendChild(menu);
    const rect = menu.getBoundingClientRect();
    if (rect.right > window.innerWidth) {
      menu.style.left = `${Math.max(0, window.innerWidth - rect.width - 4)}px`;
    }
    if (rect.bottom > window.innerHeight) {
      menu.style.top = `${Math.max(0, window.innerHeight - rect.height - 4)}px`;
    }
    contextMenu = menu;
  }

  document.addEventListener("mousedown", (event) => {
    removeContextMenu();
    if (event.button !== 0 || isEditable(event.target)) {
      return;
    }
    anchorPosition = textPositionFromPoint(event.clientX, event.clientY);
    pointerStart = { x: event.clientX, y: event.clientY };
    selecting = false;
  }, true);

  document.addEventListener("mousemove", (event) => {
    if (!anchorPosition || !pointerStart || event.buttons !== 1) {
      return;
    }
    const distance = Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y);
    if (!selecting && distance < DRAG_THRESHOLD) {
      return;
    }
    const focusPosition = textPositionFromPoint(event.clientX, event.clientY);
    if (!focusPosition || !applySelection(anchorPosition, focusPosition)) {
      return;
    }
    selecting = true;
    event.preventDefault();
  }, true);

  document.addEventListener("mouseup", (event) => {
    if (selecting) {
      event.preventDefault();
    }
    anchorPosition = null;
    pointerStart = null;
    selecting = false;
  }, true);

  document.addEventListener("keydown", (event) => {
    const primary = navigator.platform && navigator.platform.includes("Mac")
      ? event.metaKey
      : event.ctrlKey;
    if (primary && !event.altKey && !event.shiftKey && event.key.toLowerCase() === "a" && !isEditable(event.target)) {
      if (selectAllPageText()) {
        event.preventDefault();
      }
    }
  }, true);

  document.addEventListener("copy", (event) => {
    const text = selectedText();
    if (!text || !event.clipboardData) {
      return;
    }
    event.clipboardData.setData("text/plain", text);
    event.preventDefault();
  }, true);

  document.addEventListener("contextmenu", (event) => {
    if (isEditable(event.target) || !selectedText()) {
      return;
    }
    event.preventDefault();
    showContextMenu(event.clientX, event.clientY);
  }, true);

  document.addEventListener("scroll", removeContextMenu, true);
  window.addEventListener("blur", removeContextMenu);
})();
"##;

#[cfg(test)]
mod tests {
    use super::SLATE_TEXT_SELECTION_SCRIPT;

    #[test]
    fn text_selection_script_handles_selection_copy_and_context_menu() {
        assert!(SLATE_TEXT_SELECTION_SCRIPT.contains("setBaseAndExtent"));
        assert!(SLATE_TEXT_SELECTION_SCRIPT.contains("getClientRects"));
        assert!(SLATE_TEXT_SELECTION_SCRIPT.contains("navigator.clipboard.writeText"));
        assert!(SLATE_TEXT_SELECTION_SCRIPT.contains("contextmenu"));
        assert!(SLATE_TEXT_SELECTION_SCRIPT.contains("selectAllPageText"));
    }
}
