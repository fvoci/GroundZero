# Attribute observed build activity to the full measured screen window.
.[0] as $screens | .[1] as $samples |
if any($samples[]; (.epochMs | type) != "number" or
    (.compiler | type) != "boolean" or (.nodeBuild | type) != "boolean" or
    (has("heavyCommand") and (.heavyCommand | type) != "boolean"))
then error("invalid build monitor sample") else . end |
$screens | map(
  if (.sampleStart | type) != "number" or (.sampleEnd | type) != "number" or
      .sampleEnd < .sampleStart
  then error("invalid screen sample window") else . end |
  . as $screen |
  [$samples[] | select((.compiler or .nodeBuild or .heavyCommand) and
    .epochMs >= $screen.sampleStart and .epochMs <= $screen.sampleEnd)] as $overlap |
  select($overlap | length > 0) | . + {buildSamples: $overlap}
)
