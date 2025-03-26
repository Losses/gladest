#import sys: inputs
#import "@preview/mitex:0.2.5": *

#set text(font: "Noto Serif SC", size: 10pt)
#set page(fill: none, width: auto, height: auto, margin: (left: 0pt, right: 0pt, top: 0.455em, bottom: 0.455em))
#show math.equation: set text(font: "IBM Plex Math")

#let content = inputs.formula
#let inline = inputs.inline

#if inline [
  #mi(content)
] else [
  #mitex(content)
]