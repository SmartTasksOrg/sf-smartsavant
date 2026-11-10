'use strict';
// SmartSavant council engine for Node.js: the engine, gates, schema checks, adapters (stub, scripted,
// replay), exporters, importer and audit of the Python reference (src/sf_smartsavant).
const canonical = require('./canonical');
const gates = require('./gates');
const schema = require('./schema');
const model = require('./model');
const engine = require('./engine');
const adapters = require('./adapters');
const exporters = require('./exporters');
const importer = require('./importer');
const audit = require('./audit');

module.exports = {
  version: '0.1.0',
  // canonical JSON
  dumps: canonical.dumps, encode: canonical.encode, sha256Hex: canonical.sha256Hex,
  // gates
  gates,
  squash: gates.squash, blank: gates.blank, redact: gates.redact, figures: gates.figures, flagUnsourced: gates.flagUnsourced, isGeneric: gates.isGeneric,
  isFallback: gates.isFallback, hasPlaceholderCode: gates.hasPlaceholderCode, synthesisOk: gates.synthesisOk,
  complianceBlock: gates.complianceBlock,
  // schema
  checkView: schema.checkView, checkMerge: schema.checkMerge, normalizeView: schema.normalizeView,
  checkCouncil: schema.checkCouncil, plain: schema.plain, NotPlainData: schema.NotPlainData,
  // model
  model, resultFromDict: model.resultFromDict,
  // engine
  Council: engine.Council, DEFAULT_ROSTER: engine.DEFAULT_ROSTER, DEFAULT_PHASES: engine.DEFAULT_PHASES,
  loadPhases: engine.loadPhases, checkPhases: engine.checkPhases, health: engine.health,
  // adapters
  LLMAdapter: adapters.LLMAdapter, StubAdapter: adapters.StubAdapter, ScriptedAdapter: adapters.ScriptedAdapter,
  ReplayAdapter: adapters.ReplayAdapter, AdapterError: adapters.AdapterError, ReplayError: adapters.ReplayError,
  // exporters
  runDigest: exporters.runDigest, toStories: exporters.toStories, toSpecMd: exporters.toSpecMd,
  toTrainset: exporters.toTrainset, toMermaid: exporters.toMermaid,
  // importer
  fromSmartsignal: importer.fromSmartsignal, detect: importer.detect, ideaText: importer.ideaText,
  // audit
  auditResult: audit.auditResult, auditStories: audit.auditStories, auditTrainset: audit.auditTrainset,
  auditPath: audit.auditPath, readJsonl: audit.readJsonl, esc: audit.esc, render: audit.render,
};
