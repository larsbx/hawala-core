import TigerBeetleFirst
set_option maxRecDepth 16384
set_option maxHeartbeats 40000000
open TigerBeetleFirst

namespace TigerBeetleFirstExport
def alphabet : List (String × Event) := [
  ("accept(1,true)", .accept 1 true), ("accept(2,true)", .accept 2 true),
  ("accept(3,true)", .accept 3 true), ("accept(1,false)", .accept 1 false),
  ("persistA(1)", .persistA 1), ("persistA(2)", .persistA 2), ("persistA(3)", .persistA 3),
  ("persistB(1)", .persistB 1), ("persistB(2)", .persistB 2), ("persistB(3)", .persistB 3),
  ("acknowledge(1,true)", .acknowledge 1 true),
  ("acknowledge(2,true)", .acknowledge 2 true),
  ("acknowledge(3,true)", .acknowledge 3 true),
  ("acknowledge(3,false)", .acknowledge 3 false),
  ("timeout", .timeout), ("crash", .crash), ("replay", .replay)]

def path (c a b k : Nat) : List (String × Event) :=
  ((List.range c).map fun i => (s!"accept({i+1},true)", .accept (i+1) true)) ++
  ((List.range a).map fun i => (s!"persistA({i+1})", .persistA (i+1))) ++
  ((List.range b).map fun i => (s!"persistB({i+1})", .persistB (i+1))) ++
  (if k = 0 then [] else [(s!"acknowledge({k},true)", .acknowledge k true)])

def graph : List (State × List (String × Event)) :=
  (List.range 4).flatMap fun c =>
    (List.range (c+1)).flatMap fun a =>
      (List.range (c+1)).flatMap fun b =>
        (List.range (c+1)).map fun k =>
          (⟨c,c,a,b,k⟩, path c a b k)

example : graph.length = 100 := by decide
example : graph.all (fun p => decide
    (p.2.foldl (fun s e => step s e.2) initial = p.1)) = true := by decide
example : graph.all (fun p => alphabet.all (fun e =>
    graph.any (fun q => decide (q.1 = step p.1 e.2)))) = true := by decide

def fields (s : State) : List (String × String) := [
  ("committed", toString s.committed), ("effects", toString s.effects),
  ("sinkA", toString s.sinkA), ("sinkB", toString s.sinkB),
  ("acknowledged", toString s.acknowledged)]
def record (s : State) := "{ " ++ String.intercalate ", "
  ((fields s).map fun (k,v) => k ++ ": " ++ v) ++ " }"
end TigerBeetleFirstExport

open TigerBeetleFirstExport
def main (args : List String) : IO Unit := do
  let out ← IO.getStdout
  let quint := args == ["quint"]
  if quint then
    out.putStrLn "// Finite transition certificate; Lean exporter must reproduce all 1700 edges.\nmodule tigerBeetleFirstConformance {\n  import tigerBeetleFirst.* from \"./tigerBeetleFirst\""
  else out.putStrLn ("path\t" ++ String.intercalate "\t" ((fields initial).map Prod.fst))
  let mut index := 0
  for p in graph do
    for e in alphabet do
      let names := (p.2.map Prod.fst) ++ [e.1]
      let target := step p.1 e.2
      if quint then
        let acts := String.join (names.map fun name => s!".then({name})")
        out.putStrLn s!"  run tbFirstEdge{index}Test = init{acts}.expect(s == {record target})"
      else
        out.putStrLn (String.intercalate ";" names ++ "\t" ++
          String.intercalate "\t" ((fields target).map Prod.snd))
      index := index + 1
  if quint then out.putStrLn "}"
