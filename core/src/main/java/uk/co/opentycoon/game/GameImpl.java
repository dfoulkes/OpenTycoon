package uk.co.opentycoon.game;

import java.time.LocalDateTime;
import java.time.LocalTime;

public class GameImpl implements Game {

    private GameTime gameDateTime;
    private GameState gameState;

    @Override
    public void start() {
        gameDateTime =  new GameTimeImpl();
        gameState = GameState.RUNNING;
    }

    @Override
    public Integer getCurrentYear() {
        return gameDateTime.getYear();
    }

    @Override
    public LocalTime getTime() {
        return LocalTime.from(gameDateTime.getGameDateTime());
    }

    @Override
    public void tick() {
        if(gameState.equals(GameState.RUNNING))
            gameDateTime.tick();
        verifyIfEndOfGame();
    }

    @Override
    public LocalDateTime getDateTime() {
        return gameDateTime.getGameDateTime();
    }

    @Override
    public GameState getState() {
        return gameState;
    }

    private void verifyIfEndOfGame() {
        if(getCurrentYear().equals(2000)){
            gameState = GameState.END;
        }
    }

}
