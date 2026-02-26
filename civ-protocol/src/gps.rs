use crate::response::RawGpsPosition;

/// GPS position data from the radio's built-in receiver.
#[derive(Debug, Clone, Default)]
pub struct GpsPosition {
    /// Latitude in decimal degrees (negative = South).
    pub latitude: f64,
    /// Longitude in decimal degrees (negative = West).
    pub longitude: f64,
    /// Altitude in meters (negative = below sea level).
    pub altitude: f64,
    /// Course/heading in degrees (0–359).
    pub course: u16,
    /// Speed in km/h.
    pub speed: f64,
    /// UTC year.
    pub utc_year: u16,
    /// UTC month (1–12).
    pub utc_month: u8,
    /// UTC day (1–31).
    pub utc_day: u8,
    /// UTC hour (0–23).
    pub utc_hour: u8,
    /// UTC minute (0–59).
    pub utc_minute: u8,
    /// UTC second (0–59).
    pub utc_second: u8,
}

/// Convert a `RawGpsPosition` (integer BCD fields) to a `GpsPosition` (float fields).
pub fn raw_to_gps_position(raw: &RawGpsPosition) -> GpsPosition {
    // Latitude: dd + mm.mmm / 60
    let lat_minutes = raw.lat_min as f64 + raw.lat_min_frac as f64 / 1000.0;
    let mut latitude = raw.lat_deg as f64 + lat_minutes / 60.0;
    if !raw.lat_north {
        latitude = -latitude;
    }

    // Longitude: ddd + mm.mmm / 60
    let lon_minutes = raw.lon_min as f64 + raw.lon_min_frac as f64 / 1000.0;
    let mut longitude = raw.lon_deg as f64 + lon_minutes / 60.0;
    if !raw.lon_east {
        longitude = -longitude;
    }

    // Altitude in meters (0.1m resolution)
    let mut altitude = raw.alt_tenths as f64 / 10.0;
    if raw.alt_negative {
        altitude = -altitude;
    }

    GpsPosition {
        latitude,
        longitude,
        altitude,
        course: raw.course,
        speed: raw.speed_tenths as f64 / 10.0,
        utc_year: raw.utc_year,
        utc_month: raw.utc_month,
        utc_day: raw.utc_day,
        utc_hour: raw.utc_hour,
        utc_minute: raw.utc_minute,
        utc_second: raw.utc_second,
    }
}
