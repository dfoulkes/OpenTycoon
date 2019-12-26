package uk.co.opentycoon.game;

import java.time.LocalDateTime;
import java.time.LocalTime;

public class GameImpl implements Game {

    private GameTime gameDateTime;
    private GameState state;

    @Override
    public void start() {
        gameDateTime =  new GameTimeImpl();
        state = GameState.RUNNING;
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
        if(state.equals(GameState.RUNNING))
            gameDateTime.tick();
        verifyIfEndOfGame();
    }

    @Override
    public LocalDateTime getDateTime() {
        return gameDateTime.getGameDateTime();
    }

    @Override
    public GameState getState() {
        return state;
    }

    private void verifyIfEndOfGame() {
        if(getCurrentYear().equals(2000)){
            state = GameState.END;
        }
    }

}
