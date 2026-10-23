// Export the facts of the engine's member readers from a Ghidra project, for export.py.
// Arguments: out=<file> list=<file> key_text=<offset> value_token=<offset> block_token=<id>.
// Each line of the list is one query; each line of the output is one JSON object,
// {"query": ..., "result": ...} or {"query": ..., "error": ...}. The output holds names,
// addresses and typed facts. It never holds decompiled text.
//
// Queries (a target is an address, 0x..., or an exact function name):
//   program              the program's name, language, Ghidra version and SHA-256
//   names:<name>         every function with that name: entry and size
//   ctors:<class>        each constructor of the class: the vtables that it stores and the
//                        member constructors that it calls, by owner offset
//   vtslot:<addr>:<n>    the function in slot n of the vtable at addr
//   paths:<target>       the reader walk: dispatch facts and the events of each token's paths
//   enum:<target>        the tokens that a token converter compares, and whether it logs
//@category PDX

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileOptions;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.framework.Application;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressSet;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.pcode.HighFunction;
import ghidra.program.model.pcode.HighSymbol;
import ghidra.program.model.pcode.HighVariable;
import ghidra.program.model.pcode.JumpTable;
import ghidra.program.model.pcode.PcodeBlock;
import ghidra.program.model.pcode.PcodeBlockBasic;
import ghidra.program.model.pcode.PcodeOp;
import ghidra.program.model.pcode.Varnode;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.Symbol;

import java.io.PrintWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;

public class Export extends GhidraScript {
	/** Leaves that one token's walk may reach before it stops and reports itself truncated. */
	private static final int PATH_BUDGET = 512;
	private static final long UNKNOWN = Long.MIN_VALUE;
	/** The decompiler's warnings for a switch case whose label it could not compute. */
	private static final List<String> LABEL_WARNINGS =
		List.of("Bad switch case", "Calculation of case label failed", "may not be properly labeled as switch case");
	/** Functions whose call on a path means a log message. */
	private static final String[] LOG_CALLS = {"CPdxLogFileAndLine::", "CReader::ReportUnexpected", "CLogger::"};

	private final Gson gson = new Gson();
	private DecompInterface decompiler;
	private Map<String, List<Function>> byName;
	private long keyText;
	private long valueToken;
	private long blockToken;

	@Override
	protected void run() throws Exception {
		Map<String, String> arguments = new HashMap<>();
		for (String argument : getScriptArgs()) {
			int equals = argument.indexOf('=');
			arguments.put(argument.substring(0, equals), argument.substring(equals + 1));
		}
		keyText = Long.decode(arguments.get("key_text"));
		valueToken = Long.decode(arguments.get("value_token"));
		blockToken = Long.decode(arguments.get("block_token"));
		byName = new HashMap<>();
		for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
			byName.computeIfAbsent(f.getName(true), k -> new ArrayList<>()).add(f);
		}

		try (PrintWriter out = new PrintWriter(arguments.get("out"), "UTF-8")) {
			for (String line : Files.readAllLines(Path.of(arguments.get("list")))) {
				String query = line.trim();
				if (query.isEmpty()) continue;
				JsonObject answer = new JsonObject();
				answer.addProperty("query", query);
				try {
					answer.add("result", gson.toJsonTree(answer(query)));
				}
				catch (Exception e) {
					answer.addProperty("error", String.valueOf(e.getMessage()));
				}
				out.println(gson.toJson(answer));
				out.flush();
			}
		}
		finally {
			if (decompiler != null) decompiler.dispose();
		}
	}

	private Object answer(String query) throws Exception {
		int colon = query.indexOf(':');
		String kind = colon < 0 ? query : query.substring(0, colon);
		String target = colon < 0 ? "" : query.substring(colon + 1);
		return switch (kind) {
			case "program" -> program();
			case "names" -> names(target);
			case "ctors" -> constructors(target);
			case "vtslot" -> vtableSlot(target);
			case "paths" -> new ReaderWalk(function(target)).result();
			case "enum" -> tokenConverter(function(target));
			default -> throw new IllegalArgumentException("unknown query kind " + kind);
		};
	}

	private JsonObject program() {
		JsonObject result = new JsonObject();
		result.addProperty("name", currentProgram.getName());
		result.addProperty("language", currentProgram.getLanguageID().toString());
		result.addProperty("ghidra", Application.getApplicationVersion());
		result.addProperty("sha256", currentProgram.getExecutableSHA256());
		return result;
	}

	private JsonArray names(String name) {
		JsonArray result = new JsonArray();
		for (Function f : byName.getOrDefault(name, List.of())) {
			if (f.isExternal()) continue;
			JsonObject entry = new JsonObject();
			entry.addProperty("entry", hex(f.getEntryPoint()));
			entry.addProperty("size", f.getBody().getNumAddresses());
			result.add(entry);
		}
		return result;
	}

	/** Each constructor of `cls`: the vtables it stores into its object and the member constructors it calls. */
	private JsonArray constructors(String cls) {
		String simple = cls.contains("::") ? cls.substring(cls.lastIndexOf("::") + 2) : cls;
		simple = simple.replaceAll("<.*", "");
		JsonArray result = new JsonArray();
		for (Function f : byName.getOrDefault(cls + "::" + simple, List.of())) {
			if (f.isExternal()) continue;
			HighFunction high = decompile(f).getHighFunction();
			Set<Varnode> self = new HashSet<>();
			if (high.getLocalSymbolMap().getNumParams() > 0) {
				addInstances(high.getLocalSymbolMap().getParamSymbol(0).getHighVariable(), self);
			}
			JsonArray vtables = new JsonArray();
			JsonArray members = new JsonArray();
			for (PcodeBlockBasic block : high.getBasicBlocks()) {
				var ops = block.getIterator();
				while (ops.hasNext()) {
					PcodeOp op = ops.next();
					if (op.getOpcode() == PcodeOp.STORE) {
						long offset = offsetFrom(op.getInput(1), self, 0);
						long address = constantAddress(op.getInput(2), 0);
						if (offset != UNKNOWN && address != UNKNOWN && isVtable(address)) {
							JsonObject store = new JsonObject();
							store.addProperty("offset", offset);
							store.addProperty("address", "0x" + Long.toHexString(address));
							vtables.add(store);
						}
					}
					if (op.getOpcode() == PcodeOp.CALL && op.getNumInputs() > 1) {
						Function callee = getFunctionAt(op.getInput(0).getAddress());
						long offset = offsetFrom(op.getInput(1), self, 0);
						if (callee != null && offset != UNKNOWN && isConstructor(callee.getName(true))) {
							JsonObject member = new JsonObject();
							member.addProperty("offset", offset);
							String name = callee.getName(true);
							member.addProperty("class", name.substring(0, name.lastIndexOf("::")));
							members.add(member);
						}
					}
				}
			}
			JsonObject constructor = new JsonObject();
			constructor.addProperty("entry", hex(f.getEntryPoint()));
			constructor.add("vtables", vtables);
			constructor.add("members", members);
			result.add(constructor);
		}
		return result;
	}

	private JsonObject vtableSlot(String target) {
		String[] parts = target.split(":");
		Address slot = toAddr(parts[0]).add(8L * Integer.parseInt(parts[1]));
		Function f = pointedFunction(slot);
		JsonObject result = new JsonObject();
		result.addProperty("slot", hex(slot));
		result.addProperty("target", f == null ? null : hex(f.getEntryPoint()));
		result.addProperty("name", f == null ? null : f.getName(true));
		return result;
	}

	/** The tokens that a token converter such as `TokenToEnum<E>` compares its argument with. */
	private JsonObject tokenConverter(Function f) {
		HighFunction high = decompile(f).getHighFunction();
		Set<Varnode> argument = new HashSet<>();
		if (high.getLocalSymbolMap().getNumParams() > 0) {
			addInstances(high.getLocalSymbolMap().getParamSymbol(0).getHighVariable(), argument);
		}
		Set<Long> tokens = new TreeSet<>();
		boolean labelsKnown = true;
		boolean logs = false;
		for (PcodeBlockBasic block : high.getBasicBlocks()) {
			var ops = block.getIterator();
			while (ops.hasNext()) {
				PcodeOp op = ops.next();
				int code = op.getOpcode();
				if (code == PcodeOp.INT_EQUAL || code == PcodeOp.INT_NOTEQUAL) {
					for (int k = 0; k < 2; k++) {
						Varnode constant = op.getInput(1 - k);
						if (constant.isConstant() && readsArgument(op.getInput(k), argument, 0)) {
							tokens.add(signed(constant));
						}
					}
				}
				if (code == PcodeOp.CALL && isLogCall(callName(op))) logs = true;
			}
		}
		JumpTable[] tables = high.getJumpTables();
		if (tables != null) {
			for (JumpTable table : tables) {
				Integer[] labels = table.getLabelValues();
				if (labels == null) labelsKnown = false;
				else for (Integer label : labels) tokens.add((long) label);
			}
		}
		JsonObject result = new JsonObject();
		result.addProperty("entry", hex(f.getEntryPoint()));
		result.addProperty("name", f.getName(true));
		result.add("tokens", gson.toJsonTree(tokens));
		result.addProperty("labels_known", labelsKnown);
		result.addProperty("logs", logs);
		return result;
	}

	/** Whether `v` is the converter's argument, or the word that the argument points to. */
	private boolean readsArgument(Varnode v, Set<Varnode> argument, int depth) {
		if (depth > 8) return false;
		if (argument.contains(v)) return true;
		PcodeOp def = v.getDef();
		if (def == null) return false;
		return switch (def.getOpcode()) {
			case PcodeOp.COPY, PcodeOp.CAST, PcodeOp.INT_ZEXT, PcodeOp.INT_SEXT, PcodeOp.LOAD -> {
				Varnode source = def.getOpcode() == PcodeOp.LOAD ? def.getInput(1) : def.getInput(0);
				yield readsArgument(source, argument, depth + 1);
			}
			case PcodeOp.SUBPIECE -> def.getInput(1).getOffset() == 0 && readsArgument(def.getInput(0), argument, depth + 1);
			default -> false;
		};
	}

	/**
	 * One walk of a member reader over the decompiler's basic blocks. A key reader receives the
	 * statement's token as an int parameter; a value reader (`ReadEntry`, or no token parameter)
	 * tests the value token that the reader holds. The walk follows each token that the reader
	 * compares, and the default: every other token at once.
	 */
	private final class ReaderWalk {
		private final Function function;
		private final HighFunction high;
		private final Set<Varnode> thisVars = new HashSet<>();
		private final Set<Varnode> readerVars = new HashSet<>();
		private final Set<Varnode> tokenVars = new HashSet<>();
		private final boolean keyReader;
		/** Whether the decompiler warned that it could not label a switch case. */
		private final boolean labelWarning;
		private final Map<Address, PcodeBlockBasic> blockAt = new HashMap<>();
		private final Map<Address, Map<Long, Address>> switchCases = new HashMap<>();
		private final Map<Address, Address> switchDefaults = new HashMap<>();
		private final Map<PcodeBlock, PcodeBlock> predecessorOnPath = new HashMap<>();
		private final Map<PcodeBlockBasic, Integer> entryEvent = new HashMap<>();
		private final Set<Long> enumerated = new TreeSet<>();
		private int orderedCompares;
		private int budget;
		/** The walked token: `token` itself, or any token that the reader does not enumerate. */
		private long token;
		private boolean anyOtherToken;
		/** The value token on the walk: 0 untested, BLOCK, or OTHER (any token but the block's). */
		private int valueWalk;
		private static final int BLOCK = 1;
		private static final int OTHER = 2;

		ReaderWalk(Function function) {
			this.function = function;
			DecompileResults results = decompile(function);
			this.high = results.getHighFunction();
			String code = results.getDecompiledFunction() == null ? "" : results.getDecompiledFunction().getC();
			this.labelWarning = LABEL_WARNINGS.stream().anyMatch(code::contains);
			keyReader = !function.getName().equals("ReadEntry");
			assignArgumentRegisters();
			for (PcodeBlockBasic block : high.getBasicBlocks()) blockAt.put(block.getStart(), block);
		}

		/**
		 * Find `this`, the reader and the token by their argument registers (x0, x1, w2), not by the
		 * decompiler's parameter list: a prototype without `this` shifts the parameters by one and
		 * leaves the token as an unnamed input register.
		 */
		private void assignArgumentRegisters() {
			int parameters = high.getLocalSymbolMap().getNumParams();
			for (int i = 0; i < parameters; i++) {
				HighSymbol parameter = high.getLocalSymbolMap().getParamSymbol(i);
				if (parameter == null || parameter.getStorage().getFirstVarnode() == null) continue;
				int index = argumentIndex(parameter.getStorage().getFirstVarnode());
				if (parameter.getHighVariable() != null) {
					for (Varnode instance : parameter.getHighVariable().getInstances()) assignArgument(index, instance);
				}
			}
			for (PcodeBlockBasic block : high.getBasicBlocks()) {
				var ops = block.getIterator();
				while (ops.hasNext()) {
					PcodeOp op = ops.next();
					for (int i = 0; i < op.getNumInputs(); i++) {
						Varnode input = op.getInput(i);
						if (input.isInput()) assignArgument(argumentIndex(input), input);
					}
				}
			}
		}

		private void assignArgument(int index, Varnode v) {
			if (index == 0) thisVars.add(v);
			if (index == 1) readerVars.add(v);
			if (index == 2 && keyReader) tokenVars.add(v);
		}

		JsonObject result() {
			JsonArray switches = new JsonArray();
			JumpTable[] tables = high.getJumpTables();
			if (tables != null) {
				for (JumpTable table : tables) {
					Map<Long, Address> cases = new HashMap<>();
					Address[] targets = table.getCases();
					Integer[] labels = table.getLabelValues();
					for (int i = 0; labels != null && i < targets.length && i < labels.length; i++) {
						cases.put((long) labels[i], targets[i]);
					}
					// The decompiler leaves out the label of an entry with none: the default guard,
					// which it appends last, or a case whose label it could not compute, which it
					// warns about. Only a table with no such warning and at most the last entry
					// unlabelled has labels that line up with its destinations.
					int labelled = labels == null ? 0 : labels.length;
					boolean guarded = labelled == targets.length - 1;
					boolean aligned = !labelWarning && (labelled == targets.length || guarded);
					// A switch on another value than the token plus a constant, such as a masked
					// token, a lookup or a local enumeration, has labels that are no tokens: the
					// walks take each of its destinations. A label of a switch on token + k is the
					// token label - k.
					long offset = switchOffset(table.getSwitchAddress());
					boolean onToken = offset != UNKNOWN;
					if (onToken) {
						Map<Long, Address> tokenCases = new HashMap<>();
						for (Map.Entry<Long, Address> label : cases.entrySet()) {
							tokenCases.put(label.getKey() - offset, label.getValue());
						}
						cases = tokenCases;
						switchCases.put(table.getSwitchAddress(), cases);
						if (aligned && guarded) switchDefaults.put(table.getSwitchAddress(), targets[targets.length - 1]);
						enumerated.addAll(cases.keySet());
					}
					JsonObject entry = new JsonObject();
					entry.addProperty("at", hex(table.getSwitchAddress()));
					entry.addProperty("on_token", onToken);
					entry.addProperty("labels_known", aligned);
					entry.addProperty("default", guarded);
					entry.addProperty("cases", labelled);
					switches.add(entry);
				}
			}
			collectComparedTokens();

			JsonObject walks = new JsonObject();
			anyOtherToken = true;
			walks.add("default", walk(0));
			anyOtherToken = false;
			for (long t : enumerated) {
				JsonObject walked = walk(t);
				walks.add(String.valueOf(t), walked);
				if (keyReader && walked.get("value_test").getAsBoolean()) {
					valueWalk = BLOCK;
					walks.add(t + "@block", walk(t));
					valueWalk = OTHER;
					walks.add(t + "@other", walk(t));
					valueWalk = 0;
				}
			}

			JsonObject result = new JsonObject();
			result.addProperty("name", function.getName(true));
			result.addProperty("entry", hex(function.getEntryPoint()));
			result.addProperty("source", keyReader ? "key" : "value");
			result.add("switches", switches);
			result.add("compared", gson.toJsonTree(enumerated));
			result.addProperty("ordered_compares", orderedCompares);
			result.add("walks", walks);
			return result;
		}

		/**
		 * The offset k when the switch at `at` switches on the walked token plus k, or UNKNOWN. The
		 * decompiler folds a jump table's normalization into the branch, so its input is the
		 * switched value itself; only value-preserving steps may lie between it and the token.
		 */
		private long switchOffset(Address at) {
			for (PcodeBlockBasic block : high.getBasicBlocks()) {
				var ops = block.getIterator();
				while (ops.hasNext()) {
					PcodeOp op = ops.next();
					if (op.getOpcode() == PcodeOp.BRANCHIND && op.getSeqnum().getTarget().equals(at)) {
						return preservedTokenOffset(op.getInput(0), 0);
					}
				}
			}
			return UNKNOWN;
		}

		/** v == token + k through copies, widenings and constant offsets only: returns k, or UNKNOWN. */
		private long preservedTokenOffset(Varnode v, int depth) {
			if (depth > 12 || v.getSize() < 4) return UNKNOWN;
			if (keyReader ? tokenVars.contains(v) : wordOffset(v, 0, true) == 0) return 0;
			PcodeOp def = v.getDef();
			if (def == null) return UNKNOWN;
			switch (def.getOpcode()) {
				case PcodeOp.COPY, PcodeOp.CAST, PcodeOp.INT_ZEXT, PcodeOp.INT_SEXT -> {
					return preservedTokenOffset(def.getInput(0), depth + 1);
				}
				case PcodeOp.SUBPIECE -> {
					return def.getInput(1).getOffset() == 0 ? preservedTokenOffset(def.getInput(0), depth + 1) : UNKNOWN;
				}
				case PcodeOp.INT_ADD, PcodeOp.INT_SUB -> {
					if (!def.getInput(1).isConstant()) return UNKNOWN;
					long base = preservedTokenOffset(def.getInput(0), depth + 1);
					if (base == UNKNOWN) return UNKNOWN;
					long constant = signed(def.getInput(1));
					return def.getOpcode() == PcodeOp.INT_ADD ? base + constant : base - constant;
				}
				default -> {
					return UNKNOWN;
				}
			}
		}

		/** Every constant that a path compares the token with for equality; count the ordered compares. */
		private void collectComparedTokens() {
			for (PcodeBlockBasic block : high.getBasicBlocks()) {
				var ops = block.getIterator();
				while (ops.hasNext()) {
					PcodeOp op = ops.next();
					int code = op.getOpcode();
					boolean equality = code == PcodeOp.INT_EQUAL || code == PcodeOp.INT_NOTEQUAL;
					boolean ordered = code == PcodeOp.INT_SLESS || code == PcodeOp.INT_SLESSEQUAL
						|| code == PcodeOp.INT_LESS || code == PcodeOp.INT_LESSEQUAL;
					if (!equality && !ordered) continue;
					for (int k = 0; k < 2; k++) {
						Varnode constant = op.getInput(1 - k);
						long base = tokenOffset(op.getInput(k), 0);
						if (!constant.isConstant() || base == UNKNOWN) continue;
						if (equality) enumerated.add(signed(constant) - base);
						else orderedCompares++;
					}
				}
			}
		}

		private JsonObject walk(long walkedToken) {
			token = walkedToken;
			budget = PATH_BUDGET;
			predecessorOnPath.clear();
			entryEvent.clear();
			List<List<String>> leaves = new ArrayList<>();
			visit((PcodeBlockBasic) high.getBasicBlocks().get(0), null, new ArrayList<>(), new HashSet<>(), leaves);

			Set<String> union = new LinkedHashSet<>();
			Set<String> always = null;
			int unknown = 0;
			for (List<String> leaf : leaves) {
				Set<String> events = new LinkedHashSet<>();
				for (String event : leaf) {
					if (event.equals("?T") || event.equals("?F")) unknown++;
					else events.add(event);
				}
				union.addAll(events);
				if (always == null) always = new LinkedHashSet<>(events);
				else always.retainAll(events);
			}
			JsonArray events = new JsonArray();
			for (String event : union) {
				JsonArray pair = new JsonArray();
				pair.add(always != null && always.contains(event));
				pair.add(event);
				events.add(pair);
			}
			JsonObject result = new JsonObject();
			result.addProperty("leaves", leaves.size());
			result.addProperty("unknown", unknown);
			result.addProperty("truncated", budget <= 0);
			result.addProperty("value_test", union.contains("VALUETEST"));
			result.add("events", events);
			return result;
		}

		private void visit(PcodeBlockBasic block, PcodeBlock from, List<String> events, Set<PcodeBlockBasic> onPath,
				List<List<String>> leaves) {
			if (budget <= 0) return;
			if (onPath.contains(block)) {
				List<String> leaf = new ArrayList<>(events);
				leaf.add("LOOP " + String.join(";", calls(events.subList(entryEvent.get(block), events.size()))));
				leaves.add(leaf);
				budget--;
				return;
			}
			onPath.add(block);
			entryEvent.put(block, events.size());
			PcodeBlock savedPredecessor = predecessorOnPath.put(block, from);
			List<String> local = new ArrayList<>(events);
			PcodeOp last = null;
			var ops = block.getIterator();
			while (ops.hasNext()) {
				PcodeOp op = ops.next();
				last = op;
				String event = event(op);
				if (event != null && !(event.equals("KEYTEXT") && advancedStatement(local))) local.add(event);
			}
			local.addAll(dataReferences(block));

			int code = last == null ? -1 : last.getOpcode();
			if (code == PcodeOp.RETURN || block.getOutSize() == 0) {
				leaves.add(local);
				budget--;
			}
			else if (code == PcodeOp.CBRANCH) {
				long condition = eval(last.getInput(1), 0);
				if (condition == UNKNOWN) {
					if (keyReader && readsValueToken(last.getInput(1), 0)) local.add("VALUETEST");
					List<String> taken = new ArrayList<>(local);
					taken.add("?T");
					visit((PcodeBlockBasic) block.getTrueOut(), block, taken, onPath, leaves);
					List<String> fallen = new ArrayList<>(local);
					fallen.add("?F");
					visit((PcodeBlockBasic) block.getFalseOut(), block, fallen, onPath, leaves);
				}
				else {
					// A value branch walk marks where its value test decides the path, so that the
					// export can tell what the path does before the test from what the branch does.
					if (keyReader && valueWalk != 0 && readsValueToken(last.getInput(1), 0)) local.add("VALUEBRANCH");
					visit((PcodeBlockBasic) (condition != 0 ? block.getTrueOut() : block.getFalseOut()), block, local, onPath,
						leaves);
				}
			}
			else if (code == PcodeOp.BRANCHIND) {
				visitSwitch(block, last, local, onPath, leaves);
			}
			else {
				for (int i = 0; i < block.getOutSize(); i++) {
					visit((PcodeBlockBasic) block.getOut(i), block, local, onPath, leaves);
				}
			}
			onPath.remove(block);
			entryEvent.remove(block);
			if (savedPredecessor == null) predecessorOnPath.remove(block);
			else predecessorOnPath.put(block, savedPredecessor);
		}

		/**
		 * Follow the walked token's case. A token with no case follows the switch's default
		 * destination, the last of the jump table's destinations; when the table has none, every
		 * successor that is no case.
		 */
		private void visitSwitch(PcodeBlockBasic block, PcodeOp branch, List<String> local, Set<PcodeBlockBasic> onPath,
				List<List<String>> leaves) {
			Address at = branch.getSeqnum().getTarget();
			Map<Long, Address> cases = switchCases.get(at);
			Address target = cases == null || anyOtherToken ? null : cases.get(token);
			List<String> events = local;
			if (target == null && switchDefaults.containsKey(at)) {
				target = switchDefaults.get(at);
				events = new ArrayList<>(local);
				events.add("SWITCHDEFAULT");
			}
			PcodeBlockBasic chosen = target == null ? null : blockContaining(block, target);
			if (chosen != null) {
				visit(chosen, block, events, onPath, leaves);
				return;
			}
			Set<Address> caseTargets = cases == null ? Set.of() : new HashSet<>(cases.values());
			boolean any = false;
			for (int i = 0; i < block.getOutSize(); i++) {
				PcodeBlockBasic out = (PcodeBlockBasic) block.getOut(i);
				if (caseTargets.contains(out.getStart())) continue;
				any = true;
				List<String> other = new ArrayList<>(local);
				other.add("SWITCHDEFAULT");
				visit(out, block, other, onPath, leaves);
			}
			if (!any) {
				local.add("SWITCHMISS");
				leaves.add(local);
				budget--;
			}
		}

		/** The successor of `block` that contains `target`, else any block that does. */
		private PcodeBlockBasic blockContaining(PcodeBlockBasic block, Address target) {
			for (int i = 0; i < block.getOutSize(); i++) {
				PcodeBlockBasic out = (PcodeBlockBasic) block.getOut(i);
				if (out.contains(target)) return out;
			}
			for (PcodeBlockBasic candidate : blockAt.values()) if (candidate.contains(target)) return candidate;
			return null;
		}

		/** Whether the path has read another statement, after which the reader's key is that statement's. */
		private boolean advancedStatement(List<String> events) {
			for (String event : events) {
				if (event.startsWith("CALL CReader::ReadSimpleStatement") || event.startsWith("CALL CReader::ParseAdvancedStatement")) {
					return true;
				}
			}
			return false;
		}

		private List<String> calls(List<String> events) {
			List<String> names = new ArrayList<>();
			for (String event : events) {
				if (event.startsWith("CALL ")) names.add(event.substring(5).split(" \\(")[0]);
				else if (event.startsWith("CALLIND ")) names.add(event);
			}
			return names;
		}

		/** The input of a MULTIEQUAL that the current path selects, or null. */
		private Varnode selected(PcodeOp phi) {
			PcodeBlock merge = phi.getParent();
			PcodeBlock predecessor = predecessorOnPath.get(merge);
			if (predecessor == null) return null;
			for (int i = 0; i < merge.getInSize(); i++) {
				if (merge.getIn(i) == predecessor && i < phi.getNumInputs()) return phi.getInput(i);
			}
			return null;
		}

		private List<String> dataReferences(PcodeBlockBasic block) {
			List<String> references = new ArrayList<>();
			AddressSet range = new AddressSet(block.getStart(), block.getStop());
			for (Instruction instruction : currentProgram.getListing().getInstructions(range, true)) {
				for (Reference reference : instruction.getReferencesFrom()) {
					Address to = reference.getToAddress();
					if (!to.isMemoryAddress() || reference.getReferenceType().isFlow()) continue;
					Symbol symbol = getSymbolAt(to);
					String name = symbol == null ? "" : symbol.getName();
					if (!name.startsWith("PTR_") && !name.contains("vtable")) continue;
					String first = "";
					String fifth = "";
					try {
						Function slot0 = pointedFunction(to);
						first = slot0 == null ? "" : slot0.getName(true);
						Function slot5 = pointedFunction(to.add(40));
						fifth = slot5 == null ? "" : slot5.getEntryPoint() + "=" + slot5.getName(true);
					}
					catch (Exception e) {
						// The reference is not a readable pointer table.
					}
					references.add("REF " + to + " " + name + " slot0=" + first + " slot5=" + fifth);
				}
			}
			return references;
		}

		private String event(PcodeOp op) {
			int code = op.getOpcode();
			if (code == PcodeOp.CALL) {
				Address target = op.getInput(0).getAddress();
				Function callee = getFunctionAt(target);
				String name = callee == null ? target.toString() : callee.getName(true);
				String prototype = callee == null ? "" : callee.getPrototypeString(false, false).replaceAll("^[^(]*\\(", "(");
				StringBuilder arguments = new StringBuilder();
				for (int i = 1; i < op.getNumInputs(); i++) {
					Varnode input = op.getInput(i);
					long offset = offsetFrom(input, thisVars, 0, this::selected);
					if (offset != UNKNOWN) arguments.append(" a").append(i).append("=this+0x").append(Long.toHexString(offset));
					if (input.isConstant() && i > 1) arguments.append(" a").append(i).append("=#0x").append(Long.toHexString(input.getOffset()));
				}
				long scope = scopeArgument(callee, op);
				if (scope != UNKNOWN) arguments.append(" scope=#0x").append(Long.toHexString(scope));
				return "CALL " + name + " " + prototype + arguments;
			}
			if (code == PcodeOp.CALLIND) {
				boolean onReader = op.getNumInputs() > 1 && offsetFrom(op.getInput(1), readerVars, 0, this::selected) == 0;
				return "CALLIND " + indirectSlot(op.getInput(0), 0) + (onReader ? " on=reader" : "");
			}
			if (code == PcodeOp.STORE) {
				long offset = offsetFrom(op.getInput(1), thisVars, 0, this::selected);
				if (offset != UNKNOWN) {
					return "STORE this+0x" + Long.toHexString(offset) + " size=" + op.getInput(2).getSize() + " val="
						+ describe(op.getInput(2), 0);
				}
			}
			if (code == PcodeOp.LOAD && offsetFrom(op.getInput(1), readerVars, 0, this::selected) == keyText) return "KEYTEXT";
			return null;
		}

		private String indirectSlot(Varnode v, int depth) {
			PcodeOp def = v.getDef();
			if (def == null || depth > 6) return "?";
			if (def.getOpcode() == PcodeOp.LOAD) {
				PcodeOp address = def.getInput(1).getDef();
				while (address != null && (address.getOpcode() == PcodeOp.CAST || address.getOpcode() == PcodeOp.COPY)) {
					address = address.getInput(0).getDef();
				}
				if (address != null && (address.getOpcode() == PcodeOp.INT_ADD || address.getOpcode() == PcodeOp.PTRSUB
					|| address.getOpcode() == PcodeOp.PTRADD) && address.getInput(1).isConstant()) {
					return "slot+0x" + Long.toHexString(address.getInput(1).getOffset());
				}
				return "slot+0x0";
			}
			if (def.getOpcode() == PcodeOp.CAST || def.getOpcode() == PcodeOp.COPY) return indirectSlot(def.getInput(0), depth + 1);
			if (def.getOpcode() == PcodeOp.MULTIEQUAL) {
				Varnode chosen = selected(def);
				return chosen == null ? "?" : indirectSlot(chosen, depth + 1);
			}
			return "?";
		}

		/** A short description of a stored value's source. */
		private String describe(Varnode v, int depth) {
			if (v.isConstant()) return "#0x" + Long.toHexString(v.getOffset());
			PcodeOp def = v.getDef();
			if (def == null || depth > 8) return "?";
			switch (def.getOpcode()) {
				case PcodeOp.COPY, PcodeOp.CAST, PcodeOp.INT_ZEXT, PcodeOp.INT_SEXT, PcodeOp.SUBPIECE, PcodeOp.PIECE,
					PcodeOp.INT_AND, PcodeOp.INT_OR, PcodeOp.INT_LEFT, PcodeOp.INT_RIGHT -> {
					for (int i = 0; i < def.getNumInputs(); i++) {
						String described = describe(def.getInput(i), depth + 1);
						if (!described.equals("?") && !described.startsWith("#")) return described;
					}
					return "?";
				}
				case PcodeOp.MULTIEQUAL -> {
					Varnode chosen = selected(def);
					return chosen == null ? "?" : describe(chosen, depth + 1);
				}
				case PcodeOp.LOAD -> {
					long offset = offsetFrom(def.getInput(1), readerVars, 0, this::selected);
					return offset == UNKNOWN ? "load" : "reader+0x" + Long.toHexString(offset);
				}
				case PcodeOp.INT_EQUAL, PcodeOp.INT_NOTEQUAL -> {
					String left = describe(def.getInput(0), depth + 1);
					String right = describe(def.getInput(1), depth + 1);
					return (def.getOpcode() == PcodeOp.INT_EQUAL ? "eq(" : "ne(") + left + "," + right + ")";
				}
				case PcodeOp.CALL -> {
					Function callee = getFunctionAt(def.getInput(0).getAddress());
					return "call:" + (callee == null ? "?" : callee.getName(true));
				}
				default -> {
					return "op" + def.getOpcode();
				}
			}
		}

		/** Whether a condition depends on the value token that the reader holds. */
		private boolean readsValueToken(Varnode v, int depth) {
			if (depth > 8 || v.isConstant()) return false;
			PcodeOp def = v.getDef();
			if (def == null) return false;
			if (def.getOpcode() == PcodeOp.LOAD) return offsetFrom(def.getInput(1), readerVars, 0, this::selected) == valueToken;
			if (def.getOpcode() == PcodeOp.MULTIEQUAL) {
				Varnode chosen = selected(def);
				return chosen != null && readsValueToken(chosen, depth + 1);
			}
			for (int i = 0; i < def.getNumInputs(); i++) {
				if (readsValueToken(def.getInput(i), depth + 1)) return true;
			}
			return false;
		}

		/** v == token + k: returns k, or UNKNOWN. A key reader's token is its parameter; a value reader's, its value token. */
		private long tokenOffset(Varnode v, int depth) {
			return keyReader ? wordOffset(v, depth, false) : wordOffset(v, depth, true);
		}

		/** v == word + k, where the word is the token parameter or the reader's value token. */
		private long wordOffset(Varnode v, int depth, boolean value) {
			if (depth > 12) return UNKNOWN;
			if (!value && tokenVars.contains(v)) return 0;
			PcodeOp def = v.getDef();
			if (def == null) return UNKNOWN;
			if (value && def.getOpcode() == PcodeOp.LOAD
				&& offsetFrom(def.getInput(1), readerVars, 0, this::selected) == valueToken) {
				return 0;
			}
			switch (def.getOpcode()) {
				case PcodeOp.COPY, PcodeOp.CAST, PcodeOp.INT_ZEXT, PcodeOp.INT_SEXT -> {
					return wordOffset(def.getInput(0), depth + 1, value);
				}
				case PcodeOp.SUBPIECE -> {
					return def.getInput(1).isConstant() && def.getInput(1).getOffset() == 0
						? wordOffset(def.getInput(0), depth + 1, value) : UNKNOWN;
				}
				case PcodeOp.INT_ADD, PcodeOp.INT_SUB -> {
					if (!def.getInput(1).isConstant()) return UNKNOWN;
					long base = wordOffset(def.getInput(0), depth + 1, value);
					if (base == UNKNOWN) return UNKNOWN;
					long constant = signed(def.getInput(1));
					return def.getOpcode() == PcodeOp.INT_ADD ? base + constant : base - constant;
				}
				default -> {
					return UNKNOWN;
				}
			}
		}

		/**
		 * The value of v on the current path, or UNKNOWN. On the default walk the token is any
		 * token that the reader does not enumerate: an equality with an enumerated token is false
		 * and every other test of it is unknown. On an OTHER walk the value token is any token but
		 * the block's.
		 */
		private long eval(Varnode v, int depth) {
			if (depth > 16) return UNKNOWN;
			if (v.isConstant()) return signed(v);
			long k = tokenOffset(v, 0);
			if (k != UNKNOWN) return anyOtherToken ? UNKNOWN : token + k;
			if (keyReader && valueWalk == BLOCK) {
				long value = wordOffset(v, 0, true);
				if (value != UNKNOWN) return blockToken + value;
			}
			PcodeOp def = v.getDef();
			if (def == null) return UNKNOWN;
			int code = def.getOpcode();
			if (code == PcodeOp.INT_EQUAL || code == PcodeOp.INT_NOTEQUAL) {
				long excluded = excludedEquality(def, depth);
				if (excluded != UNKNOWN) return excluded;
			}
			if (code == PcodeOp.BOOL_NEGATE) {
				long operand = eval(def.getInput(0), depth + 1);
				return operand == UNKNOWN ? UNKNOWN : (operand == 0 ? 1 : 0);
			}
			if (code == PcodeOp.COPY || code == PcodeOp.CAST) return eval(def.getInput(0), depth + 1);
			if (code == PcodeOp.MULTIEQUAL) {
				Varnode chosen = selected(def);
				return chosen == null ? UNKNOWN : eval(chosen, depth + 1);
			}
			if (code == PcodeOp.BOOL_AND || code == PcodeOp.BOOL_OR) {
				long left = eval(def.getInput(0), depth + 1);
				long right = eval(def.getInput(1), depth + 1);
				if (code == PcodeOp.BOOL_AND) {
					if (left == 0 || right == 0) return 0;
					return left == UNKNOWN || right == UNKNOWN ? UNKNOWN : 1;
				}
				if (left == 1 || right == 1) return 1;
				return left == UNKNOWN || right == UNKNOWN ? UNKNOWN : 0;
			}
			if (def.getNumInputs() != 2) return UNKNOWN;
			Varnode x = def.getInput(0);
			long left = eval(x, depth + 1);
			long right = eval(def.getInput(1), depth + 1);
			if (left == UNKNOWN || right == UNKNOWN) return UNKNOWN;
			int size = x.getSize();
			return switch (code) {
				case PcodeOp.INT_EQUAL -> mask(left, size) == mask(right, size) ? 1 : 0;
				case PcodeOp.INT_NOTEQUAL -> mask(left, size) != mask(right, size) ? 1 : 0;
				case PcodeOp.INT_SLESS -> left < right ? 1 : 0;
				case PcodeOp.INT_SLESSEQUAL -> left <= right ? 1 : 0;
				case PcodeOp.INT_LESS -> Long.compareUnsigned(mask(left, size), mask(right, size)) < 0 ? 1 : 0;
				case PcodeOp.INT_LESSEQUAL -> Long.compareUnsigned(mask(left, size), mask(right, size)) <= 0 ? 1 : 0;
				default -> UNKNOWN;
			};
		}

		/** An equality that the walked token's exclusions decide: 0 or 1, or UNKNOWN. */
		private long excludedEquality(PcodeOp def, int depth) {
			for (int k = 0; k < 2; k++) {
				long constant = eval(def.getInput(1 - k), depth + 1);
				if (constant == UNKNOWN) continue;
				Varnode tested = def.getInput(k);
				boolean unequal = false;
				long offset = tokenOffset(tested, 0);
				if (anyOtherToken && offset != UNKNOWN && enumerated.contains(constant - offset)) unequal = true;
				long value = keyReader && valueWalk == OTHER ? wordOffset(tested, 0, true) : UNKNOWN;
				if (value != UNKNOWN && constant - value == blockToken) unequal = true;
				if (unequal) return def.getOpcode() == PcodeOp.INT_EQUAL ? 0 : 1;
			}
			return UNKNOWN;
		}
	}

	// ---- shared helpers ----

	private interface Selector {
		Varnode select(PcodeOp phi);
	}

	private long offsetFrom(Varnode v, Set<Varnode> base, int depth) {
		return offsetFrom(v, base, depth, phi -> null);
	}

	/** v == base + k: returns k, or UNKNOWN. */
	private long offsetFrom(Varnode v, Set<Varnode> base, int depth, Selector selector) {
		if (depth > 12) return UNKNOWN;
		if (base.contains(v)) return 0;
		PcodeOp def = v.getDef();
		if (def == null) return UNKNOWN;
		switch (def.getOpcode()) {
			case PcodeOp.COPY, PcodeOp.CAST, PcodeOp.INT_ZEXT -> {
				return offsetFrom(def.getInput(0), base, depth + 1, selector);
			}
			case PcodeOp.PTRSUB, PcodeOp.INT_ADD -> {
				if (!def.getInput(1).isConstant()) return UNKNOWN;
				long start = offsetFrom(def.getInput(0), base, depth + 1, selector);
				return start == UNKNOWN ? UNKNOWN : start + signed(def.getInput(1));
			}
			case PcodeOp.PTRADD -> {
				if (!def.getInput(1).isConstant() || !def.getInput(2).isConstant()) return UNKNOWN;
				long start = offsetFrom(def.getInput(0), base, depth + 1, selector);
				return start == UNKNOWN ? UNKNOWN : start + signed(def.getInput(1)) * def.getInput(2).getOffset();
			}
			case PcodeOp.MULTIEQUAL -> {
				Varnode chosen = selector.select(def);
				if (chosen != null) return offsetFrom(chosen, base, depth + 1, selector);
				long agreed = UNKNOWN;
				for (int i = 0; i < def.getNumInputs(); i++) {
					long offset = offsetFrom(def.getInput(i), base, depth + 1, selector);
					if (offset == UNKNOWN || (agreed != UNKNOWN && agreed != offset)) return UNKNOWN;
					agreed = offset;
				}
				return agreed;
			}
			default -> {
				return UNKNOWN;
			}
		}
	}

	/** The address that a stored value names, when it is a constant: a literal or a global's address. */
	private long constantAddress(Varnode v, int depth) {
		if (depth > 4) return UNKNOWN;
		if (v.isConstant()) return v.getOffset();
		if (v.isAddress()) return UNKNOWN;
		PcodeOp def = v.getDef();
		if (def == null) return UNKNOWN;
		if (def.getOpcode() == PcodeOp.COPY || def.getOpcode() == PcodeOp.CAST) return constantAddress(def.getInput(0), depth + 1);
		if (def.getOpcode() == PcodeOp.PTRSUB && def.getInput(0).isConstant() && def.getInput(0).getOffset() == 0
			&& def.getInput(1).isConstant()) {
			return def.getInput(1).getOffset();
		}
		return UNKNOWN;
	}

	private boolean isVtable(long address) {
		try {
			Symbol symbol = getSymbolAt(toAddr(address));
			return symbol != null && (symbol.getName().startsWith("PTR_") || symbol.getName().contains("vtable"));
		}
		catch (Exception e) {
			return false;
		}
	}

	/**
	 * The literal value of a call's `EScopeType` argument, or UNKNOWN. The parameters are matched
	 * to the call's inputs from the last one, because a prototype may leave out `this`.
	 */
	private long scopeArgument(Function callee, PcodeOp call) {
		if (callee == null) return UNKNOWN;
		var parameters = callee.getParameters();
		for (int i = 0; i < parameters.length; i++) {
			if (!parameters[i].getDataType().getName().contains("EScopeType")) continue;
			int input = call.getNumInputs() - (parameters.length - i);
			if (input >= 1 && call.getInput(input).isConstant()) return call.getInput(input).getOffset();
		}
		return UNKNOWN;
	}

	/** The argument number of a register varnode, x0 to x7 (or its w half), or -1. */
	private int argumentIndex(Varnode v) {
		if (!v.isRegister()) return -1;
		var register = currentProgram.getRegister(v.getAddress(), v.getSize());
		if (register == null) return -1;
		String name = register.getBaseRegister().getName();
		if (!name.matches("x[0-7]")) return -1;
		return Integer.parseInt(name.substring(1));
	}

	private static boolean isConstructor(String name) {
		String[] parts = name.replaceAll("<[^<>]*(?:<[^<>]*>[^<>]*)*>", "").split("::");
		return parts.length >= 2 && parts[parts.length - 1].equals(parts[parts.length - 2]);
	}

	private static boolean isLogCall(String name) {
		for (String log : LOG_CALLS) if (name.startsWith(log)) return true;
		return false;
	}

	private String callName(PcodeOp call) {
		Function callee = getFunctionAt(call.getInput(0).getAddress());
		return callee == null ? "" : callee.getName(true);
	}

	/** The function that a pointer at `slot` points to, through a thunk. */
	private Function pointedFunction(Address slot) {
		try {
			long value = getLong(slot);
			Function f = getFunctionAt(toAddr(value & 0xFFFFFFFFFL | 0x100000000L));
			return f != null && f.isThunk() ? f.getThunkedFunction(true) : f;
		}
		catch (Exception e) {
			return null;
		}
	}

	private DecompileResults decompile(Function f) {
		if (decompiler == null) {
			decompiler = new DecompInterface();
			decompiler.setOptions(new DecompileOptions());
			decompiler.toggleCCode(true);
			decompiler.toggleSyntaxTree(true);
			decompiler.openProgram(currentProgram);
		}
		DecompileResults results = decompiler.decompileFunction(f, 120, monitor);
		if (!results.decompileCompleted() || results.getHighFunction() == null) {
			throw new IllegalStateException("decompile failed: " + results.getErrorMessage());
		}
		return results;
	}

	/** The function at an address or with an exact name; the largest body wins among overloads. */
	private Function function(String target) {
		if (target.startsWith("0x")) {
			Function f = getFunctionContaining(toAddr(target));
			if (f == null) throw new IllegalArgumentException("no function contains " + target);
			return f.isThunk() ? f.getThunkedFunction(true) : f;
		}
		String name = target.replaceAll("\\(.*\\)$", "");
		Function best = null;
		for (Function f : byName.getOrDefault(name, List.of())) {
			if (f.isExternal()) continue;
			if (best == null || f.getBody().getNumAddresses() > best.getBody().getNumAddresses()) best = f;
		}
		if (best == null) throw new IllegalArgumentException("no function named " + name);
		return best.isThunk() ? best.getThunkedFunction(true) : best;
	}

	private static void addInstances(HighVariable variable, Set<Varnode> set) {
		if (variable == null) return;
		for (Varnode instance : variable.getInstances()) set.add(instance);
	}

	private static String hex(Address address) {
		return "0x" + address.toString();
	}

	private static long signed(Varnode constant) {
		long value = constant.getOffset();
		int bits = constant.getSize() * 8;
		if (bits < 64) {
			value &= (1L << bits) - 1;
			if ((value & (1L << (bits - 1))) != 0) value -= 1L << bits;
		}
		return value;
	}

	private static long mask(long value, int size) {
		return size >= 8 ? value : value & ((1L << (size * 8)) - 1);
	}
}
