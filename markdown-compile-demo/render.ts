import { writeFileSync } from "node:fs";

import MarkdownIt from "markdown-it";

import markdownItGladest from "markdown-it-gladest";

const md = new MarkdownIt({
  html: true,
  xhtmlOut: true,
  breaks: false,
  typographer: true,
}).use(markdownItGladest, {
  format: "svg",
  ppi: 600,
});

const document = `
# Normal Distribution: A Mathematical Overview

The Normal distribution, also known as the Gaussian distribution, is one of the most important probability distributions in statistics. It is characterized by its bell-shaped curve and is completely determined by two parameters: the mean (μ) and the standard deviation (σ).

## Mathematical Definition

The probability density function (PDF) of a normal distribution is given by:

$$
f(x) = \\frac{1}{\\sigma\\sqrt{2\\pi}} e^{-\\frac{1}{2}\\left(\\frac{x-\\mu}{\\sigma}\\right)^2}
$$

Where:
- $\\mu$ is the mean (expected value)
- $\\sigma$ is the standard deviation
- $\\sigma^2$ is the variance

## Key Properties

1. The distribution is symmetric around its mean $\\mu$, which is also the median and mode.

2. About 68% of the data falls within one standard deviation $\\sigma$ of the mean, 95% within two standard deviations, and 99.7% within three standard deviations. This is often called the "68-95-99.7 rule" or the "empirical rule."

3. The standard normal distribution has $\\mu = 0$ and $\\sigma = 1$, denoted as $Z \\sim N(0,1)$.

4. If $X \\sim N(\\mu, \\sigma^2)$, then $Z = \\frac{X-\\mu}{\\sigma} \\sim N(0,1)$. This transformation is called "standardization."

5. The moment generating function (MGF) is:

$$
M_X(t) = e^{\\mu t + \\frac{1}{2}\\sigma^2 t^2}
$$

## Applications

The normal distribution is widely used in:

- Natural sciences to represent random variables whose distributions are not known
- Financial analysis for modeling asset returns
- Quality control in manufacturing
- Error analysis in experimental measurements

The central limit theorem explains why the normal distribution appears so frequently in nature: the sum of a large number of independent random variables tends toward a normal distribution, regardless of the original distributions of the variables.
`;

writeFileSync(".output.html", md.render(document));
