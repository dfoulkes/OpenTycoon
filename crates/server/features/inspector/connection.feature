# Spec §10d (R88): single player is the real server on 127.0.0.1 (R44), streaming over
# WebSocket (docs/inspector-protocol.md). The client only sends commands (R40).
Feature: The inspector connection
  The server binds localhost on a port the OS picks, greets a client that knows the protocol
  version and the one-time token, starts a game on request, changes speed and closes when the
  client leaves.

  Background:
    Given an inspector server started on the shipped config with a fresh token

  @R88 @R44
  Scenario: The server listens on localhost only
    Then the server listens on 127.0.0.1 on a port the OS chose

  @R88 @R44
  Scenario: A client with the right token and protocol is welcomed
    When a client connects and says hello with protocol 1 and the token
    Then the client receives "welcome" with protocol 1

  @R88 @R44
  Scenario: A client with the wrong token is refused
    When a client connects and says hello with protocol 1 and the token "00000000000000000000000000000000"
    Then the client receives an error naming "bad token"
    And the connection is closed

  @R88
  Scenario: A client speaking another protocol version is refused
    When a client connects and says hello with protocol 0 and the token
    Then the client receives an error naming "protocol 0 is not supported"
    And the connection is closed

  @R88 @R40
  Scenario: A spectator starts a game, speeds it up and leaves
    When a client connects and says hello with protocol 1 and the token
    And the client sends new_game with seed 42
    Then the client receives a snapshot of 81 lots at tick 0
    When the client sends set_speed 4
    Then the client receives a delta at speed 4 with a later tick
    When the client sends set_speed 9
    Then the client receives an error naming "speed must be 0 to 4"
    When the client sends leave
    Then the connection is closed
    And the server thread ends cleanly
