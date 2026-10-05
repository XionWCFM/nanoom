#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# Validate public composite Action metadata. Runtime contracts below check behavior.
ruby - <<'RUBY'
require 'yaml'
Dir.glob('.github/actions/*/action.yml').each do |path|
  action = YAML.load_file(path)
  abort "#{path}: expected composite Action" unless action.dig('runs', 'using') == 'composite'
  next if File.basename(File.dirname(path)).start_with?('_')
  action.fetch('inputs', {}).each do |name, input|
    abort "#{path}: #{name} needs a description" unless input['description'].is_a?(String) && !input['description'].empty?
  end
  ids = action.dig('runs', 'steps').map { |step| step['id'] }.compact
  action.fetch('outputs', {}).each do |name, output|
    match = output.fetch('value').match(/steps\.([\w-]+)\.outputs\./)
    input = output.fetch('value').match(/inputs\.([\w-]+)/)
    valid = (match && ids.include?(match[1])) || (input && action.fetch('inputs', {}).key?(input[1]))
    abort "#{path}: #{name} references a missing step or input" unless valid
  end
end
# GHES uses the v3 artifact protocol; dotcom uses v4. Patch versions are not contracts.
{'upload' => %w[affected run history], 'download' => %w[prepare]}.each do |operation, names|
  names.each do |name|
    [name, "#{name}-ghes"].each do |variant|
      action = YAML.load_file(".github/actions/#{variant}/action.yml")
      major = variant.end_with?('-ghes') ? 'v3' : 'v4'
      transfers = action.dig('runs', 'steps').map { |step| step['uses'] }.compact.select { |use| use.start_with?("actions/#{operation}-artifact@") }
      abort "#{variant}: wrong artifact protocol" unless !transfers.empty? && transfers.all? { |use| use.match?(/@#{major}(?:\.|$)/) }
    end
  end
end
RUBY

bash scripts/status-action-test.sh
bash scripts/coordinator-contract-test.sh
# Includes native Windows checkout paths, canonical workspace boundaries, and
# independent verification that preparation digests match the planner's JSON bytes.
bash scripts/assignment-action-test.sh
bash scripts/preparation-clock-test.sh
# Cover both unbudgeted release artifact listing and bounded history reads.
bash scripts/history-artifact-test.sh
bash scripts/history-ghes-test.sh
bash scripts/setup-smoke.sh
bash scripts/plan-action-test.sh
# Prove installation, not only the shape of a package-manager stub command.
bash scripts/focused-install-test.sh
# Includes opened/synchronize/push/merge_group expression checks and shallow base fetch.
bash scripts/revision-action-test.sh
bash scripts/cleanup-checkout-test.sh
bash scripts/fixture-completion-test.sh
echo 'action contract passed'
