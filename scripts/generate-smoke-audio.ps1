param([switch]$Coherent)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force (Join-Path $PSScriptRoot '../artifacts') | Out-Null
Add-Type -AssemblyName System.Speech
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$format = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
$fixtureName = if ($Coherent) { '../artifacts/coherent-speech.wav' } else { '../artifacts/smoke-speech.wav' }
$fixtureText = if ($Coherent) { 'Not only can we increase the strength of these deep sleep brain waves, but we can also nearly double the memory benefit that people get from sleep. The question now is whether we can bring this affordable and portable technology into their homes.' } else { 'Could you tell me about your experience in software engineering?' }
$synth.SetOutputToWaveFile((Join-Path $PSScriptRoot $fixtureName), $format)
$synth.Speak($fixtureText)
$synth.Dispose()
