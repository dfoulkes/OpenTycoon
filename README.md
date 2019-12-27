## Open Tycoon

## What is it?

I'm a huge fan of an early 00's game Monopoly Tycoon (MT), although the sun has set on this title a long time ago I am looking to bring it back. This time as an open source community driven initiative.

The plan is to keep the integrity of the game, for legal reasons, we will not be using any of the copyrights for obvious reasons, one addition I propose though is a much more robust multiplayer to allow for a much better online multiplayer experience.

## How to Get involved ?

I'm one person, I will give this as much time as a I can, however always looking for extra hands wherever that be writing stories for review, code reviews or writing code.

## Technology Choices

This is a odd one I know but I've chosen Java. The main reason for this is I like Java haha. No really, Java has a reasonable low entry point for developer with decent support and C like syntax without some of the complexities of C++ I know some of you will point out I'm making my life harder in writing a game engine but I think in this case the game mechanics deserve a rewrite rather then a rehash , besides if it's good enough for Minecraft then surely it's good enough for this game.


## Build

## Requirements 
  - OpenJDK8
  - maven 

To build:
```bash
 mvn clean install package
```

The project uses JaCoCo to manage the test coverage, this means if you write any code that does not have 80% test coverage the build will break, thankfully it also comes with 
a quite helpful built in reporting tool, currently after a build it will generate a report in: 
```bash
${project.dir}/${module}/site/jacoco/index.html
```


## Testing
In the interest of keeping a high quality  code, I've been using a BDD approach for ticket creation with them all stories written with gherkin annotations, as such I'm looking 
to propagate this down into the source itself both for readability of the code and ensure we capture all the game play. As the project matures so will the tests.
