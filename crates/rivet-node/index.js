"use strict";

const path = require("node:path");

const nativeAddon = path.join(
  __dirname,
  `rivet_node.${process.platform}-${process.arch}.node`,
);

module.exports = require(nativeAddon);
