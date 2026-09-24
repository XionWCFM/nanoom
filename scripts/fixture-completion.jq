def valid_plan:
  . as $p
  | ($p.version == 1)
    and (($p.groups | type) == "object")
    and (($p.assignmentCount | type) == "number" and $p.assignmentCount >= 0)
    and (($p.itemCount | type) == "number" and $p.itemCount >= 0)
    and ([$p.groups[]?.assignments[]?] | length) == $p.assignmentCount
    and ([$p.groups[]?.assignments[]?.items[]?] | length) == $p.itemCount
    and all($p.groups[]?.assignments[]?; (.items | type) == "array" and (.items | length) > 0)
    and ($p.hasChange == ($p.itemCount > 0))
    and (if $p.hasChange then $p.assignmentCount > 0 else $p.assignmentCount == 0 end);

($plan | length) == 1
and ($plan[0] | valid_plan)
and ([.[] | select(.name == "affected" and .conclusion == "success")] | length) == 1
and ([.[] | select(.name == "status" and .conclusion == "success")] | length) == 1
and (
  [.[] | select(.name == "run" or (.name | startswith("run (")))] as $runs
  | if $plan[0].hasChange then
      ($runs | length) == $plan[0].assignmentCount and all($runs[]; .conclusion == "success")
    else
      all($runs[]; .conclusion == "skipped")
    end
)
