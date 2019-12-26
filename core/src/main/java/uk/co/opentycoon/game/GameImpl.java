package uk.co.opentycoon.game;

import uk.co.opentycoon.GameState;

import java.time.LocalDate;
import java.time.LocalDateTime;
import java.time.LocalTime;

public class GameImpl implements Game {

    private LocalDateTime gameDateTime;
    private GameState gameState;

    @Override
    public void start() {
        LocalDate localDate = LocalDate.of(1930,1, 1);
        LocalTime localTime = LocalTime.of(0,0);
        gameDateTime =  LocalDateTime.of(localDate, localTime);
        gameState = GameState.RUNNING;
    }

    @Override
    public Integer getCurrentYear() {
        return gameDateTime.getYear();
    }

    @Override
    public LocalTime getTime() {
        return LocalTime.from(gameDateTime);
    }

    @Override
    public void tick() {
        if(gameState.equals(GameState.RUNNING))
            incrementTimeByOneMinute();
    }

    @Override
    public LocalDateTime getDateTime() {
        return gameDateTime;
    }

    @Override
    public GameState getState() {
        return gameState;
    }

    private void incrementTimeByOneMinute() {
        LocalTime currentTime = LocalTime.from(gameDateTime);
        currentTime =  currentTime.plusMinutes(1);
        incrementDays(currentTime);
        determineIfEndOfYear();
        gameDateTime =  LocalDateTime.of(LocalDate.from(gameDateTime), currentTime);
    }

    private void incrementDays(LocalTime currentTime) {
        if(isExactlyMidnight(currentTime)){
            gameDateTime =  gameDateTime.plusDays(1);
        }
    }

    private void determineIfEndOfYear() {
        if(gameDateTime.getDayOfMonth() == 3){
            gameDateTime =  gameDateTime.plusYears(5L);
            gameDateTime= gameDateTime.minusDays(2L);
        }
        verifyIfEndOfGame();
    }

    private void verifyIfEndOfGame() {
        if(getCurrentYear().equals(2000)){
            gameState = GameState.END;
        }
    }

    private boolean isExactlyMidnight(LocalTime currentTime) {
        return currentTime.getHour() == 0 && currentTime.getMinute() == 0;
    }
}
