// Import Rocket Macros globally using #[macro_use]
// Rocket's main job is to route incoming requests to appropriate handler
#[macro_use]
extern crate rocket;

use rocket::form::Form;
use rocket::fs::{relative, FileServer};
use rocket::response::stream::{Event, EventStream};
use rocket::serde::{Deserialize, Serialize};
use rocket::tokio::select;
use rocket::tokio::sync::broadcast::{channel, error::RecvError, Sender};
use rocket::{Shutdown, State};

// Defines a /world route and how it handles a get request
// -- No arguements, just returns a string slice = "Hello World!"
#[get("/world")]
fn world() -> &'static str {
    "Hello World!"
}

// This struct defines the format of our messages which will be passed in our channel
// 3 fields with some validations
// Derives a few traits
// -- Debug -> Can output in debug format
// -- Clone -> Can duplicate messages
// -- FromForm -> Can take form data and translate to a message struct
// -- Serialize
// -- Deserialize
#[derive(Debug, Clone, FromForm, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")] // Serialize and Deserialize via serde (Defined in Rocket)
struct Message {
    #[field(validate = len(..30))]
    pub room: String,
    #[field(validate = len(..20))]
    pub username: String,
    pub message: String,
}

// Endpoint to Send Messages
// This endpoint will respond to post requests at /message and accepts form data
// The handler accepts two arguements, form data contiaining the message and the Sender
// Rocket will automatically convert the response into an HTTP response (response will depend on the Responder trait implementation)
// -- In this case, Result is a primitive type which implements the Responder trait
#[post("/message", data = "<form>")]
fn post(form: Form<Message>, queue: &State<Sender<Message>>) {
    // Send fails if there are no active subscribers
    let _res = queue.send(form.into_inner());
}

// Endpoint to Recieve Messages
// This async endpoint will be used to get previously-posted messages for reading
// -- Essentially will be pulling from a stream of events posted to the server by our other route
// Return Type is of type EventStream which is essentially a Stream that can get opened and listened to by a client
// -- Similar to WebSockets, except it is uni-directional (client cannot send data back to stream/server)
// Two arguements, queue and Shutdown
// -- Shutdown is a "future" which resolves when server shutsdown ("Futures" in Rust are Promises in JavaScript)
#[get("/events")]
async fn events(queue: &State<Sender<Message>>, mut end: Shutdown) -> EventStream![] {
    // Create new reciever to listen to stream of messages
    let mut rx = queue.subscribe();

    // Infinite loop to generate server sent events
    EventStream! {
        // Looping operation
        loop {
            let msg = select! {
                // Recieve a message from the stream and match it against one of the three possibilities
                msg = rx.recv() => match msg {
                    Ok(msg) => msg,                         // Proper Message
                    Err(RecvError::Closed) => break,        // Recieved Error that no more senders exist for stream
                    Err(RecvError::Lagged(_)) => continue,  // Recieved Error that our reciever lagged too far behind
                },

                // Waiting for the Shutdown future to resolve
                // When it does, break the loop
                _ = &mut end => break,
            };

            // Yield a new event and pass the message we recieved from the Stream
            yield Event::json(&msg);
        }
    }
}

// Launch Attribute
// Inside function, we call build function on rocket instance to start up app
// Before doing this, our instance will need to mount any specified routes at the base route
// -- i.e the below would create a valid route at http://127.0.0.1:8000/hello/world
#[launch]
fn rocket() -> _ {
    rocket::build()
        // Use Manage to add state to the rocket instance (all handlers have access to this instance)
        // The specific state we want to add is the sender end of a channel (to pass messages between async tasks)
        // We create a channel and then specify the type of struct we want to pass and how much we want the channel to retain
        // ".0" specifies we only want to retain the sender end of the channel
        .manage(channel::<Message>(1024).0)
        .mount("/", routes![world, post, events]) // Uses routes macro to create a list of routes
        .mount("/", FileServer::from(relative!("static"))) // Specifies where to retrieve static files from
}
