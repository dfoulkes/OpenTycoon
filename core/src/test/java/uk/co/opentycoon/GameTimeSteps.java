package uk.co.opentycoon;

import cucumber.api.java.en.And;
import cucumber.api.java.en.Given;
import cucumber.api.java.en.Then;
import cucumber.api.java.en.When;
import uk.co.opentycoon.game.Game;
import uk.co.opentycoon.game.GameImpl;

import java.time.LocalDateTime;
import java.time.LocalTime;

import static org.hamcrest.MatcherAssert.assertThat;
import static org.hamcrest.core.Is.is;

public class GameTimeSteps {

    public static final int MAX_NUMBER_OF_DAYS_IN_A_GAME = 28;
    private Game game;

    @Given("the game has started")
    public void theGameHasStarted() {
     game = new GameImpl();
     game.start();
    }

    @When("in the year nineteenThirty")
    public void inTheYearNineteenThirty() {
        assertThat("expected current year tobe 1930", game.getCurrentYear(), is(1930));
    }

    @Then("clock time should be midnight")
    public void clockTimeShouldBeMidnight() {
        assertThat("expected current hour to be 0 ", game.getTime().getHour(), is(0));
        assertThat("expected current minute to be 1 ", game.getTime().getMinute(), is(0));
    }

    @Given("end of day one of nineteenThirty")
    public void endOfDayOneOfNineteenThirty() {
        incrementToEndOFDay();
        assertThat("expected to be end of day", game.getTime(), is(LocalTime.of(23,59)));
    }

    @When("clock time is eleven fifty nine")
    public void clockTimeIsElevenFiftyNine() {
        assertThat("expected the clock to be 23:59", game.getTime(), is(LocalTime.of(23,59)));
    }

    @Then("the start of the new day should be the same year as before")
    public void theStartOfTheNewDayShouldBeTheSameYearAsBefore() {
        assertThat("expected the year to be 1930", game.getCurrentYear(), is(1930));
    }

    @Given("end of day two of nineteenThirty")
    public void endOfDayTwoOfNineteenThirty() {
        game = new GameImpl();
        game.start();
        incrementToEndOFDay();
        game.tick();
        incrementToEndOFDay();
        assertThat("expected the date tobe to be 01/01/1930 11:59", game.getDateTime(), is(LocalDateTime.of(1930,1,2,23,59)));
    }

    @Then("the start of the new day should be Nineteen Thirty Five")
    public void theStartOfTheNewDayShouldBeNineteenThirtyFive() {
        game.tick();
        assertThat("expected the current year to be 1935", game.getCurrentYear(), is(1935));
        assertThat("expected the clock to be 00:00", game.getTime(), is(LocalTime.of(0,0)));
        assertThat("expected the LocalDateTime to be 1st Jan 1935 00:00", game.getDateTime(), is(LocalDateTime.of(1935,1,1,0,0)));
    }


    @When("clock time is midnight in the year two thousand on day one")
    public void clockTimeIsMidnightInTheYearTwoThousandOnDayOne() {
        for(int day = 0; day < MAX_NUMBER_OF_DAYS_IN_A_GAME; day++){
            incrementEntireDay();
        }
        assertThat("expected to be the end of day 1 in the year 2000", game.getDateTime(), is(LocalDateTime.of(2000,1,1,0,0)));
    }

    @Then("game should end")
    public void gameShouldEnd() {
        assertThat("expected the game state to be ended", game.getState(), is(GameState.END));
    }

    @When("the game has lapsed three days")
    public void theGameHasLapsedThreeDays() {
        for(int day=0; day < 3; day++){
            incrementEntireDay();
        }
    }

    @Then("game date should be {string}")
    public void gameDateShouldBe(String args) {
        assertThat("expected to be in the second day of 1935", game.getDateTime(), is(LocalDateTime.of(1935,1,2,0,0)));
    }


    private void incrementToEndOFDay() {
        for(int hour =0; hour <= 23; hour++)
            for (int minute = 0; minute <= 59; minute++) {
                if (minute == 59 && hour == 23) break;
                game.tick();
            }
    }

    private void incrementEntireDay(){
        incrementToEndOFDay();
        game.tick();
    }

    @And("game state is running")
    public void gamerStateIsStarted() {
        assertThat("expected the name state to be running", game.getState(), is(GameState.RUNNING));
    }
}
